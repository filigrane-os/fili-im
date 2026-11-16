// The Google Messages side: libgm's session, pairing, events → protocol events, commands → the phone.
package main

import (
	"context"
	"encoding/base64"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"sync"
	"time"

	"github.com/rs/zerolog"
	"go.mau.fi/util/exhttp"

	"go.mau.fi/mautrix-gmessages/pkg/libgm"
	"go.mau.fi/mautrix-gmessages/pkg/libgm/events"
	"go.mau.fi/mautrix-gmessages/pkg/libgm/gmproto"
	"go.mau.fi/mautrix-gmessages/pkg/libgm/util"
)

const (
	commandTimeout = 45 * time.Second
	qrLifetime     = 30 * time.Second
	syncChats      = 60 // conversations listed at connection
	syncMessages   = 40 // recent messages fetched for each
	syncParallel   = 4
)

type Connector struct {
	out  *Out
	log  zerolog.Logger
	path string // auth.json

	mu       sync.Mutex
	client   *libgm.Client
	auth     *libgm.AuthData
	pairing  context.CancelFunc // set while a QR pairing runs
	syncedAt string             // session id of the last sync
	syncing  bool

	convs    sync.Map // conversation id -> *gmproto.Conversation
	lastMsg  sync.Map // conversation id -> id of its latest message
	sentIDs  sync.Map // client_id -> tmp id (idempotent sends)
	tmpToCl  sync.Map // tmp id -> client_id (echo)
	myReact  sync.Map // message id -> emoji of the user's own reaction
	announce sync.Map
}

func NewConnector(out *Out, dataDir string) (*Connector, error) {
	log := zerolog.New(os.Stderr).Level(zerolog.WarnLevel).With().Timestamp().Str("component", "libgm").Logger()
	if os.Getenv("FILI_IM_DEBUG") != "" {
		log = log.Level(zerolog.InfoLevel)
	}
	c := &Connector{out: out, log: log, path: filepath.Join(dataDir, "auth.json")}
	if raw, err := os.ReadFile(c.path); err == nil {
		var ad libgm.AuthData
		if err := json.Unmarshal(raw, &ad); err == nil && ad.Mobile != nil {
			c.auth = &ad
		}
	}
	return c, nil
}

func (c *Connector) saveAuth() {
	c.mu.Lock()
	ad := c.auth
	c.mu.Unlock()
	if ad == nil {
		return
	}
	raw, err := json.Marshal(ad)
	if err != nil {
		return
	}
	tmp := c.path + ".tmp"
	if os.WriteFile(tmp, raw, 0o600) == nil {
		_ = os.Rename(tmp, c.path)
	}
}

func (c *Connector) newClient(ad *libgm.AuthData) *libgm.Client {
	cli := libgm.NewClient(ad, nil, c.log, exhttp.SensibleClientSettings)
	cli.SetEventHandler(c.handle)
	return cli
}

func (c *Connector) cli() *libgm.Client {
	c.mu.Lock()
	defer c.mu.Unlock()
	return c.client
}

func (c *Connector) Start() {
	if c.auth == nil {
		c.out.State("needs_pairing", "", nil)
		return
	}
	c.out.State("connecting", "", nil)
	go c.connect()
}

func (c *Connector) connect() {
	c.mu.Lock()
	cli := c.newClient(c.auth)
	c.client = cli
	c.mu.Unlock()
	if err := cli.Connect(context.Background()); err != nil {
		c.log.Warn().Err(err).Msg("connect failed")
		if errors.Is(err, events.ErrInvalidCredentials) || strings.Contains(err.Error(), "refresh auth token") {
			c.forget("The phone no longer knows this computer; link it again")
			return
		}
		c.out.State("error", "Could not connect to Google Messages", nil)
	}
}

func (c *Connector) Stop() {
	c.mu.Lock()
	cli, cancel := c.client, c.pairing
	c.mu.Unlock()
	if cancel != nil {
		cancel()
	}
	if cli != nil {
		cli.SetEventHandler(nil)
		cli.Disconnect()
	}
}

// forget drops the session: the phone unlinked this computer (or the credentials are gone).
func (c *Connector) forget(detail string) {
	c.mu.Lock()
	cli := c.client
	c.client, c.auth = nil, nil
	c.mu.Unlock()
	if cli != nil {
		cli.SetEventHandler(nil)
		cli.Disconnect()
	}
	_ = os.Remove(c.path)
	c.out.State("logged_out", detail, nil)
}

func (c *Connector) self() map[string]any {
	self := map[string]any{"id": "me", "name": "Google Messages"}
	c.mu.Lock()
	ad := c.auth
	c.mu.Unlock()
	if ad != nil && ad.Mobile != nil && ad.Mobile.GetSourceID() != "" {
		self["id"] = ad.Mobile.GetSourceID()
	}
	return self
}

// ---- Events from the phone -------------------------------------------------------------------------------------

func (c *Connector) handle(raw any) {
	switch evt := raw.(type) {
	case *events.PairSuccessful:
		c.mu.Lock()
		if c.pairing != nil {
			c.pairing()
			c.pairing = nil
		}
		if c.client != nil && c.auth == nil {
			c.auth = c.client.AuthData
		}
		c.mu.Unlock()
		c.saveAuth()
		c.out.State("connecting", "", nil)
	case *events.AuthTokenRefreshed:
		c.saveAuth()
	case *events.GaiaLoggedOut:
		c.forget("Signed out of Google Messages")
	case *gmproto.RevokePairData:
		c.forget("This computer was unlinked from Google Messages on the phone")
	case *events.ListenFatalError:
		c.log.Warn().Err(evt.Error).Msg("listen failed")
		c.out.State("error", "The connection to Google Messages stopped", nil)
	case *events.ListenTemporaryError:
		c.out.State("connecting", "", nil)
	case *events.ListenRecovered:
		c.out.State("connected", "", c.self())
	case *events.PhoneNotResponding:
		c.out.State("phone_offline", "The phone is not answering (is it on and online?)", nil)
	case *events.PhoneRespondingAgain:
		c.out.State("connected", "", c.self())
	case *gmproto.UserAlertEvent:
		c.alert(evt)
	case *gmproto.Conversation:
		c.conversation(evt)
	case *libgm.WrappedMessage:
		c.message(evt.Message, !evt.IsOld)
	case *gmproto.TypingData:
		c.typing(evt)
	}
}

func (c *Connector) alert(evt *gmproto.UserAlertEvent) {
	switch evt.GetAlertType() {
	case gmproto.AlertType_BROWSER_ACTIVE:
		c.out.State("connected", "", c.self())
		cli := c.cli()
		if cli == nil {
			return
		}
		session := cli.CurrentSessionID()
		c.mu.Lock()
		again := c.syncedAt == session || c.syncing
		if !again {
			c.syncing, c.syncedAt = true, session
		}
		c.mu.Unlock()
		if !again {
			go c.sync(cli)
		}
	case gmproto.AlertType_BROWSER_INACTIVE:
		c.out.State("phone_offline", "Google Messages is open in another browser; open it here to use it again", nil)
		// Take the session back (the user chose this computer by linking it).
		go func() {
			if cli := c.cli(); cli != nil {
				_ = cli.SetActiveSession(context.Background())
			}
		}()
	}
}

func chatKind(conv *gmproto.Conversation) string {
	if conv.GetIsGroupChat() {
		return "group"
	}
	return "direct"
}

// conversation announces a conversation (title, unread, pin) and its people.
func (c *Connector) conversation(conv *gmproto.Conversation) {
	id := conv.GetConversationID()
	if id == "" {
		return
	}
	c.convs.Store(id, conv)
	if conv.GetLatestMessageID() != "" {
		c.lastMsg.Store(id, conv.GetLatestMessageID())
	}
	unread, pinned, archived := int64(0), conv.GetPinned(), conv.GetStatus() == gmproto.ConversationStatus_ARCHIVED
	if conv.GetUnread() {
		unread = 1
	}
	title := conv.GetName()
	c.out.Event("chat.upsert", map[string]any{"chat": Chat{
		ID: id, Kind: chatKind(conv), Title: title, Unread: &unread, Pinned: &pinned, Archived: &archived,
	}})
	for _, p := range conv.GetParticipants() {
		if p.GetIsMe() || !p.GetIsVisible() || p.GetID().GetParticipantID() == "" {
			continue
		}
		name := p.GetFullName()
		if name == "" {
			name = p.GetFirstName()
		}
		phone := p.GetID().GetNumber()
		key := p.GetID().GetParticipantID() + "|" + name + "|" + phone
		if _, seen := c.announce.Load(key); seen {
			continue
		}
		c.announce.Store(key, true)
		if name == "" {
			name = phone
		}
		c.out.Event("contact.upsert", map[string]any{"contact": Contact{ID: p.GetID().GetParticipantID(), Name: name, Phone: phone}})
	}
}

// who maps a participant id of a conversation to the contact id announced for that person (the phone lists some
// people twice, once invisible).
func (c *Connector) who(conv string, pid string) string {
	v, ok := c.convs.Load(conv)
	if !ok {
		return pid
	}
	var number string
	for _, p := range v.(*gmproto.Conversation).GetParticipants() {
		if !p.GetIsVisible() && p.GetID().GetParticipantID() == pid {
			number = p.GetID().GetNumber()
		}
	}
	if number == "" {
		return pid
	}
	for _, p := range v.(*gmproto.Conversation).GetParticipants() {
		if p.GetIsVisible() && p.GetID().GetNumber() == number {
			return p.GetID().GetParticipantID()
		}
	}
	return pid
}

func mediaKind(mime string) string {
	switch {
	case strings.HasPrefix(mime, "image/"):
		return "image"
	case strings.HasPrefix(mime, "video/"):
		return "video"
	case strings.HasPrefix(mime, "audio/"):
		return "audio"
	}
	return "file"
}

// convert turns a phone message into a protocol message. ok is false for what is not a message (tombstones).
func (c *Connector) convert(m *gmproto.Message) (Message, bool) {
	status := m.GetMessageStatus().GetStatus()
	if status >= 200 && status < 300 {
		return Message{}, false
	}
	// Statuses 1-99 are outgoing, 100-199 incoming; anything else is told by the participant.
	fromMe := status >= 1 && status < 100
	if status < 1 || status >= 200 {
		fromMe = m.GetParticipantID() == "1"
		if v, ok := c.convs.Load(m.GetConversationID()); ok {
			for _, p := range v.(*gmproto.Conversation).GetParticipants() {
				if p.GetIsMe() && p.GetID().GetParticipantID() == m.GetParticipantID() {
					fromMe = true
				}
			}
		}
	}
	msg := Message{
		ID: m.GetMessageID(), Chat: m.GetConversationID(), FromMe: fromMe,
		Timestamp: m.GetTimestamp() / 1000, Kind: "text",
	}
	if !fromMe {
		msg.Sender = c.who(m.GetConversationID(), m.GetParticipantID())
	}
	if r := m.GetReplyMessage(); r != nil {
		msg.ReplyTo = r.GetMessageID()
	}
	var texts []string
	for _, part := range m.GetMessageInfo() {
		if t := part.GetMessageContent(); t != nil && t.GetContent() != "" {
			texts = append(texts, t.GetContent())
		}
		if mc := part.GetMediaContent(); mc != nil && msg.Media == nil {
			kind := mediaKind(mc.GetMimeType())
			msg.Kind = kind
			msg.Media = &Media{
				Kind: kind, Mime: mc.GetMimeType(), Name: mc.GetMediaName(), Size: uint64(mc.GetSize()),
				Ref: mc.GetMediaID() + "|" + base64.RawURLEncoding.EncodeToString(mc.GetDecryptionKey()),
			}
		}
	}
	if len(texts) > 0 {
		msg.Text = &Text{Plain: strings.Join(texts, "\n")}
	}
	if msg.Text == nil && msg.Media == nil {
		return Message{}, false
	}
	if fromMe {
		if id, ok := c.tmpToCl.Load(m.GetTmpID()); ok {
			msg.ClientID = id.(string)
		}
	}
	return msg, true
}

func (c *Connector) message(m *gmproto.Message, live bool) {
	if m.GetMessageStatus().GetStatus() == gmproto.MessageStatusType_MESSAGE_DELETED {
		c.out.Event("message.delete", map[string]any{"chat": m.GetConversationID(), "id": m.GetMessageID()})
		return
	}
	msg, ok := c.convert(m)
	if !ok {
		return
	}
	c.lastMsg.Store(msg.Chat, msg.ID)
	c.out.Event("message.new", map[string]any{"live": live, "message": msg})
	c.reactions(m, msg)
}

func (c *Connector) reactions(m *gmproto.Message, msg Message) {
	for _, r := range m.GetReactions() {
		emoji := r.GetData().GetUnicode()
		for _, pid := range r.GetParticipantIDs() {
			mine := false
			if v, ok := c.convs.Load(msg.Chat); ok {
				for _, p := range v.(*gmproto.Conversation).GetParticipants() {
					if p.GetIsMe() && p.GetID().GetParticipantID() == pid {
						mine = true
					}
				}
			}
			if mine {
				c.myReact.Store(msg.ID, emoji)
			}
			ev := map[string]any{"chat": msg.Chat, "message": msg.ID, "from_me": mine, "emoji": emoji, "timestamp": msg.Timestamp}
			if !mine {
				ev["sender"] = c.who(msg.Chat, pid)
			}
			c.out.Event("reaction", ev)
		}
	}
}

func (c *Connector) typing(t *gmproto.TypingData) {
	conv := t.GetConversationID()
	number := t.GetUser().GetNumber()
	v, ok := c.convs.Load(conv)
	if !ok || number == "" {
		return
	}
	for _, p := range v.(*gmproto.Conversation).GetParticipants() {
		if p.GetIsVisible() && !p.GetIsMe() && p.GetID().GetNumber() == number {
			c.out.Event("typing", map[string]any{"chat": conv, "sender": p.GetID().GetParticipantID(), "active": t.GetType() == gmproto.TypingTypes_STARTED_TYPING})
			return
		}
	}
}

// sync lists the conversations, then fetches the latest messages of each so the window is not empty.
func (c *Connector) sync(cli *libgm.Client) {
	defer func() {
		c.mu.Lock()
		c.syncing = false
		c.mu.Unlock()
	}()
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Minute)
	defer cancel()
	resp, err := cli.ListConversations(ctx, &gmproto.ListConversationsRequest{Count: syncChats, Folder: gmproto.ListConversationsRequest_INBOX})
	if err != nil {
		c.log.Warn().Err(err).Msg("listing conversations failed")
		c.out.Event("error", map[string]any{"message": "Could not list the Google Messages conversations; will retry"})
		c.mu.Lock()
		c.syncedAt = ""
		c.mu.Unlock()
		return
	}
	convs := resp.GetConversations()
	for _, conv := range convs {
		c.conversation(conv)
	}
	c.out.State("connected", "", c.self())
	total, done := len(convs), 0
	var mu sync.Mutex
	sem := make(chan struct{}, syncParallel)
	var wg sync.WaitGroup
	for _, conv := range convs {
		wg.Add(1)
		sem <- struct{}{}
		go func(id string) {
			defer wg.Done()
			defer func() { <-sem }()
			c.history(ctx, cli, id)
			mu.Lock()
			done++
			n := done
			mu.Unlock()
			if n%10 == 0 || n == total {
				c.out.Event("history.batch", map[string]any{"messages": []Message{}, "progress": map[string]any{"done": n, "total": total}})
			}
		}(conv.GetConversationID())
	}
	wg.Wait()
}

func (c *Connector) history(ctx context.Context, cli *libgm.Client, conv string) {
	resp, err := cli.FetchMessages(ctx, conv, syncMessages, nil)
	if err != nil {
		c.log.Warn().Err(err).Msg("fetching messages failed")
		return
	}
	var batch []Message
	var latest string
	var latestTS int64
	for _, m := range resp.GetMessages() {
		if m.GetMessageStatus().GetStatus() == gmproto.MessageStatusType_MESSAGE_DELETED {
			continue
		}
		msg, ok := c.convert(m)
		if !ok {
			continue
		}
		if msg.Timestamp >= latestTS {
			latest, latestTS = msg.ID, msg.Timestamp
		}
		batch = append(batch, msg)
	}
	if latest != "" {
		c.lastMsg.Store(conv, latest)
	}
	sort.Slice(batch, func(i, j int) bool { return batch[i].Timestamp < batch[j].Timestamp })
	if len(batch) > 0 {
		c.out.Event("history.batch", map[string]any{"messages": batch})
	}
	for _, m := range resp.GetMessages() {
		if msg, ok := c.convert(m); ok {
			c.reactions(m, msg)
		}
	}
}

// ---- Commands from the daemon ----------------------------------------------------------------------------------

type cmdError struct {
	code    string
	message string
}

func (e *cmdError) Error() string { return e.message }

func fail(code, format string, args ...any) error {
	return &cmdError{code: code, message: fmt.Sprintf(format, args...)}
}

func (c *Connector) Run(cmd *Command) (any, error) {
	ctx, cancel := context.WithTimeout(context.Background(), commandTimeout)
	defer cancel()
	switch cmd.Cmd {
	case "pair":
		if cmd.Method != "" && cmd.Method != "qr" {
			return nil, fail("invalid", "only QR pairing is available")
		}
		return nil, c.pair()
	case "logout":
		return nil, c.logout(ctx)
	case "shutdown":
		return nil, nil
	case "send", "delete", "react", "mark_read", "typing", "fetch_media":
	default:
		return nil, fail("unsupported", "unknown command %q", cmd.Cmd)
	}
	cli := c.cli()
	if cli == nil || !cli.IsLoggedIn() {
		return nil, fail("not_connected", "not linked to Google Messages")
	}
	if cmd.Cmd != "fetch_media" && !cli.IsConnected() {
		return nil, fail("not_connected", "not connected to Google Messages")
	}
	switch cmd.Cmd {
	case "send":
		return c.send(ctx, cli, cmd)
	case "delete":
		resp, err := cli.DeleteMessage(ctx, cmd.Message)
		if err != nil {
			return nil, fail("failed", "could not delete: %v", err)
		}
		if !resp.GetSuccess() {
			return nil, fail("failed", "the phone refused to delete the message")
		}
		return nil, nil
	case "react":
		return nil, c.react(ctx, cli, cmd)
	case "mark_read":
		id := ""
		if n := len(cmd.Messages); n > 0 {
			id = cmd.Messages[n-1]
		} else if v, ok := c.lastMsg.Load(cmd.Chat); ok {
			id = v.(string)
		}
		if cmd.Chat == "" || id == "" {
			return nil, fail("invalid", "nothing to mark as read")
		}
		return nil, cli.MarkRead(ctx, cmd.Chat, id)
	case "typing":
		if !cmd.Active {
			return nil, nil
		}
		return nil, cli.SetTyping(ctx, cmd.Chat, c.sim(cmd.Chat))
	case "fetch_media":
		return c.fetchMedia(ctx, cli, cmd)
	}
	return nil, fail("unsupported", "unknown command %q", cmd.Cmd)
}

func (c *Connector) sim(conv string) *gmproto.SIMPayload {
	if v, ok := c.convs.Load(conv); ok {
		return v.(*gmproto.Conversation).GetSimCard().GetSIMData().GetSIMPayload()
	}
	return nil
}

func (c *Connector) send(ctx context.Context, cli *libgm.Client, cmd *Command) (any, error) {
	if cmd.Chat == "" {
		return nil, fail("invalid", "bad chat id")
	}
	if cmd.ClientID != "" {
		if tmp, ok := c.sentIDs.Load(cmd.ClientID); ok {
			return map[string]any{"id": tmp, "timestamp": time.Now().UnixMilli()}, nil
		}
	}
	caption := ""
	if cmd.Text != nil {
		caption = cmd.Text.Plain
	}
	var info []*gmproto.MessageInfo
	if len(cmd.Media) > 0 && string(cmd.Media) != "null" {
		var media struct {
			Path string `json:"path"`
			Mime string `json:"mime"`
			Name string `json:"name"`
		}
		if err := json.Unmarshal(cmd.Media, &media); err != nil || media.Path == "" {
			return nil, fail("invalid", "bad media")
		}
		data, err := os.ReadFile(media.Path)
		if err != nil {
			return nil, fail("invalid", "cannot read the file")
		}
		if media.Mime == "" {
			media.Mime = http.DetectContentType(data)
		}
		if media.Name == "" {
			media.Name = filepath.Base(media.Path)
		}
		up, err := cli.UploadMedia(data, media.Name, media.Mime)
		if err != nil {
			return nil, fail("failed", "upload failed: %v", err)
		}
		info = append(info, &gmproto.MessageInfo{Data: &gmproto.MessageInfo_MediaContent{MediaContent: up}})
	}
	if strings.TrimSpace(caption) != "" {
		info = append(info, &gmproto.MessageInfo{Data: &gmproto.MessageInfo_MessageContent{MessageContent: &gmproto.MessageContent{Content: caption}}})
	}
	if len(info) == 0 {
		return nil, fail("invalid", "empty message")
	}
	conv, _ := c.convs.Load(cmd.Chat)
	outgoing := ""
	var rcs bool
	if cv, ok := conv.(*gmproto.Conversation); ok {
		outgoing = cv.GetDefaultOutgoingID()
		rcs = cv.GetType() == gmproto.ConversationType_RCS && cv.GetSendMode() == gmproto.ConversationSendMode_SEND_MODE_AUTO
	}
	tmp := util.GenerateTmpID()
	req := &gmproto.SendMessageRequest{
		ConversationID: cmd.Chat,
		MessagePayload: &gmproto.MessagePayload{
			TmpID: tmp, TmpID2: tmp, ConversationID: cmd.Chat, ParticipantID: outgoing, MessageInfo: info,
		},
		SIMPayload: c.sim(cmd.Chat),
		TmpID:      tmp,
		ForceRCS:   rcs,
	}
	if cmd.ReplyTo != "" {
		req.Reply = &gmproto.ReplyPayload{MessageID: cmd.ReplyTo}
	}
	if cmd.ClientID != "" {
		c.sentIDs.Store(cmd.ClientID, tmp)
		c.tmpToCl.Store(tmp, cmd.ClientID)
	}
	resp, err := cli.SendMessage(ctx, req)
	if err != nil {
		c.forgetSend(cmd.ClientID, tmp)
		return nil, fail("failed", "could not send: %v", err)
	}
	if resp.GetStatus() != gmproto.SendMessageResponse_SUCCESS {
		c.forgetSend(cmd.ClientID, tmp)
		return nil, fail("failed", "the phone refused the message (%s)", resp.GetStatus())
	}
	return map[string]any{"id": tmp, "timestamp": time.Now().UnixMilli()}, nil
}

func (c *Connector) forgetSend(clientID, tmp string) {
	if clientID != "" {
		c.sentIDs.Delete(clientID)
	}
	c.tmpToCl.Delete(tmp)
}

func (c *Connector) react(ctx context.Context, cli *libgm.Client, cmd *Command) error {
	if cmd.Message == "" {
		return fail("invalid", "no message")
	}
	prev, had := c.myReact.Load(cmd.Message)
	var req *gmproto.SendReactionRequest
	switch {
	case cmd.Emoji == "" && !had:
		return nil
	case cmd.Emoji == "":
		req = &gmproto.SendReactionRequest{MessageID: cmd.Message, ReactionData: gmproto.MakeReactionData(prev.(string)), Action: gmproto.SendReactionRequest_REMOVE}
	default:
		action := gmproto.SendReactionRequest_ADD
		if had {
			action = gmproto.SendReactionRequest_SWITCH
		}
		req = &gmproto.SendReactionRequest{MessageID: cmd.Message, ReactionData: gmproto.MakeReactionData(cmd.Emoji), Action: action, SIMPayload: c.sim(cmd.Chat)}
	}
	resp, err := cli.SendReaction(ctx, req)
	if err != nil {
		return fail("failed", "could not react: %v", err)
	}
	if !resp.GetSuccess() {
		return fail("failed", "the phone refused the reaction")
	}
	if cmd.Emoji == "" {
		c.myReact.Delete(cmd.Message)
	} else {
		c.myReact.Store(cmd.Message, cmd.Emoji)
	}
	return nil
}

func (c *Connector) fetchMedia(ctx context.Context, cli *libgm.Client, cmd *Command) (any, error) {
	id, key, ok := strings.Cut(cmd.Ref, "|")
	if !ok || id == "" || cmd.Dest == "" {
		return nil, fail("invalid", "bad media reference")
	}
	raw, err := base64.RawURLEncoding.DecodeString(key)
	if err != nil || len(raw) == 0 {
		return nil, fail("invalid", "bad media key")
	}
	body, err := cli.DownloadMedia(id, raw)
	if err != nil {
		return nil, fail("failed", "download failed: %v", err)
	}
	defer body.Close()
	tmp := cmd.Dest + ".part"
	f, err := os.OpenFile(tmp, os.O_CREATE|os.O_WRONLY|os.O_TRUNC, 0o600)
	if err != nil {
		return nil, fail("failed", "cannot write the file")
	}
	n, err := io.Copy(f, body)
	f.Close()
	if err != nil || n == 0 {
		_ = os.Remove(tmp)
		return nil, fail("expired", "the file is no longer available")
	}
	if err := os.Rename(tmp, cmd.Dest); err != nil {
		return nil, fail("failed", "cannot write the file")
	}
	return map[string]any{"path": cmd.Dest}, nil
}

// pair links this computer like a browser: a QR code to scan from Google Messages > Device pairing, renewed every
// 30 seconds until the phone confirms.
func (c *Connector) pair() error {
	c.mu.Lock()
	if c.pairing != nil {
		c.mu.Unlock()
		return nil
	}
	if c.client != nil {
		c.client.SetEventHandler(nil)
		c.client.Disconnect()
		c.client = nil
	}
	ad := libgm.NewAuthData()
	cli := c.newClient(ad)
	ctx, cancel := context.WithCancel(context.Background())
	c.client, c.auth, c.pairing = cli, nil, cancel
	c.mu.Unlock()
	if err := cli.FetchConfig(ctx); err != nil {
		cancel()
		c.mu.Lock()
		c.pairing = nil
		c.mu.Unlock()
		return fail("failed", "cannot reach Google Messages: %v", err)
	}
	qr, err := cli.StartLogin(ctx)
	if err != nil {
		cancel()
		c.mu.Lock()
		c.pairing = nil
		c.mu.Unlock()
		return fail("failed", "cannot start the pairing: %v", err)
	}
	c.out.Event("pairing", map[string]any{"kind": "qr", "data": qr, "expires_s": int(qrLifetime.Seconds())})
	go func() {
		for tries := 0; tries < 10; tries++ {
			select {
			case <-ctx.Done():
				return
			case <-time.After(qrLifetime):
			}
			next, err := cli.RefreshPhoneRelay()
			if err != nil {
				c.log.Warn().Err(err).Msg("QR refresh failed")
				break
			}
			c.out.Event("pairing", map[string]any{"kind": "qr", "data": next, "expires_s": int(qrLifetime.Seconds())})
		}
		select {
		case <-ctx.Done():
		default:
			cancel()
			c.mu.Lock()
			c.pairing = nil
			c.client = nil
			c.mu.Unlock()
			cli.SetEventHandler(nil)
			cli.Disconnect()
			c.out.State("needs_pairing", "The pairing timed out; link it again", nil)
		}
	}()
	return nil
}

func (c *Connector) logout(ctx context.Context) error {
	if cli := c.cli(); cli != nil && cli.IsLoggedIn() {
		_ = cli.Unpair(ctx) // the phone drops this computer from "Device pairing"
	}
	c.forget("")
	return nil
}
