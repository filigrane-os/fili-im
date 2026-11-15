// The WhatsApp side: whatsmeow's session, pairing, events → protocol events, commands → WhatsApp.
package main

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"time"

	"go.mau.fi/whatsmeow"
	"go.mau.fi/whatsmeow/appstate"
	"go.mau.fi/whatsmeow/proto/waCommon"
	"go.mau.fi/whatsmeow/proto/waCompanionReg"
	"go.mau.fi/whatsmeow/proto/waE2E"
	"go.mau.fi/whatsmeow/store"
	"go.mau.fi/whatsmeow/store/sqlstore"
	"go.mau.fi/whatsmeow/types"
	"go.mau.fi/whatsmeow/types/events"
	waLog "go.mau.fi/whatsmeow/util/log"
	"google.golang.org/protobuf/proto"
)

const (
	commandTimeout = 30 * time.Second
	historyChunk   = 400
)

type Connector struct {
	out       *Out
	log       waLog.Logger
	container *sqlstore.Container
	local     *Local
	streams   streamer

	// Users already announced to the daemon with their phone number (and push name), so unknown numbers get a title.
	announced sync.Map

	mu      sync.Mutex
	client  *whatsmeow.Client
	pairing context.CancelFunc // set while a QR/code pairing runs
}

func init() {
	// What the phone shows under "Linked devices".
	store.SetOSInfo("Filigrane", [3]uint32{0, 1, 0})
	store.DeviceProps.PlatformType = waCompanionReg.DeviceProps_DESKTOP.Enum()
}

func NewConnector(out *Out, dataDir string, log waLog.Logger) (*Connector, error) {
	ctx := context.Background()
	container, err := sqlstore.New(ctx, "sqlite3", "file:"+dataDir+"/whatsmeow.db?_foreign_keys=on&_busy_timeout=5000&_journal_mode=WAL", log.Sub("Store"))
	if err != nil {
		return nil, fmt.Errorf("cannot open the session store: %w", err)
	}
	local, err := OpenLocal(dataDir + "/connector.db")
	if err != nil {
		return nil, fmt.Errorf("cannot open connector.db: %w", err)
	}
	device, err := container.GetFirstDevice(ctx)
	if err != nil {
		return nil, fmt.Errorf("cannot read the session: %w", err)
	}
	c := &Connector{out: out, log: log, container: container, local: local}
	c.setClient(device)
	return c, nil
}

func (c *Connector) setClient(device *store.Device) {
	client := whatsmeow.NewClient(device, c.log.Sub("Client"))
	client.EnableAutoReconnect = true
	client.InitialAutoReconnect = true
	client.AutomaticMessageRerequestFromPhone = true
	client.AddEventHandler(c.handle)
	c.mu.Lock()
	c.client = client
	c.mu.Unlock()
}

func (c *Connector) cli() *whatsmeow.Client {
	c.mu.Lock()
	defer c.mu.Unlock()
	return c.client
}

// Start: connect a paired session, or wait for `pair`.
func (c *Connector) Start() {
	cli := c.cli()
	if cli.Store.ID == nil {
		c.out.State("needs_pairing", "", nil)
		return
	}
	c.out.State("connecting", "", nil)
	if err := cli.Connect(); err != nil {
		c.out.State("error", "Cannot connect to WhatsApp: "+err.Error(), nil)
	}
}

func (c *Connector) Stop() {
	c.cli().Disconnect()
	c.local.Close()
}

// ---------------------------------------------------------------------------------------------------------------
// Identity

func (c *Connector) canon(j types.JID) types.JID {
	j = j.ToNonAD()
	if j.Server == types.HiddenUserServer {
		ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
		defer cancel()
		if pn, err := c.cli().Store.LIDs.GetPNForLID(ctx, j); err == nil && !pn.IsEmpty() {
			return pn.ToNonAD()
		}
	}
	return j
}

func phoneOf(j types.JID) string {
	if j.Server == types.DefaultUserServer {
		return "+" + j.User
	}
	return ""
}

func (c *Connector) self() map[string]any {
	cli := c.cli()
	if cli.Store.ID == nil {
		return nil
	}
	own := cli.Store.ID.ToNonAD()
	self := map[string]any{"id": own.String(), "phone": phoneOf(own)}
	if cli.Store.PushName != "" {
		self["name"] = cli.Store.PushName
	}
	return self
}

// ---------------------------------------------------------------------------------------------------------------
// Events from WhatsApp

func (c *Connector) handle(raw any) {
	switch evt := raw.(type) {
	case *events.Connected:
		c.out.State("connected", "", c.self())
		go c.syncDirectory()
	case *events.KeepAliveRestored:
		c.out.State("connected", "", c.self())
	case *events.Disconnected:
		c.out.State("connecting", "Reconnecting to WhatsApp", nil)
	case *events.KeepAliveTimeout:
		if evt.ErrorCount >= 2 {
			c.out.State("connecting", "WhatsApp is not answering; reconnecting", nil)
		}
	case *events.LoggedOut:
		c.forget("The session was ended (from the phone or by WhatsApp); pair again: " + evt.Reason.String())
	case *events.TemporaryBan:
		c.out.State("banned", evt.String(), nil)
	case *events.StreamReplaced:
		c.out.State("error", "Another program took over this WhatsApp session", nil)
	case *events.ClientOutdated:
		c.out.State("error", "WhatsApp rejects this connector as outdated; update fili-im", nil)
	case *events.ConnectFailure:
		c.out.State("error", fmt.Sprintf("WhatsApp refused the connection (%s)", evt.Reason.String()), nil)
	case *events.Message:
		c.message(evt, true)
	case *events.UndecryptableMessage:
		if chatSkipped(evt.Info.Chat) {
			return
		}
		m := Message{
			ID: evt.Info.ID, Chat: c.canon(evt.Info.Chat).String(), FromMe: evt.Info.IsFromMe,
			Timestamp: evt.Info.Timestamp.UnixMilli(), Kind: "unsupported",
			Text: &Text{Plain: "Waiting for this message. Open WhatsApp on the phone to receive it."},
		}
		if !evt.Info.IsFromMe {
			m.Sender = c.canon(evt.Info.Sender).String()
		}
		c.out.Event("message.new", map[string]any{"live": true, "message": m})
	case *events.Receipt:
		c.receipt(evt)
	case *events.ChatPresence:
		if evt.IsFromMe || chatSkipped(evt.Chat) {
			return
		}
		c.out.Event("typing", map[string]any{
			"chat": c.canon(evt.Chat).String(), "sender": c.canon(evt.Sender).String(),
			"active": evt.State == types.ChatPresenceComposing,
		})
	case *events.HistorySync:
		c.history(evt)
	case *events.PushName:
		c.pushName(evt.JID, evt.NewPushName)
	case *events.Contact:
		if evt.Action != nil {
			name := evt.Action.GetFullName()
			if name == "" {
				name = evt.Action.GetFirstName()
			}
			c.contact(evt.JID, name)
		}
	case *events.GroupInfo:
		if evt.Name != nil {
			c.out.Event("chat.upsert", map[string]any{"chat": Chat{ID: evt.JID.String(), Kind: "group", Title: evt.Name.Name}})
		}
	case *events.JoinedGroup:
		c.out.Event("chat.upsert", map[string]any{"chat": Chat{ID: evt.JID.String(), Kind: "group", Title: evt.GroupInfo.Name}})
	case *events.Mute:
		until := int64(0)
		if evt.Action.GetMuted() {
			until = evt.Action.GetMuteEndTimestamp()
			if until == 0 {
				until = -1
			}
		}
		c.chatState(evt.JID, func(ch *Chat) { ch.MutedUntil = &until })
	case *events.Archive:
		archived := evt.Action.GetArchived()
		c.chatState(evt.JID, func(ch *Chat) { ch.Archived = &archived })
	case *events.Pin:
		pinned := evt.Action.GetPinned()
		c.chatState(evt.JID, func(ch *Chat) { ch.Pinned = &pinned })
	case *events.MarkChatAsRead:
		if evt.Action.GetRead() {
			zero := int64(0)
			c.chatState(evt.JID, func(ch *Chat) { ch.Unread = &zero })
		}
	}
}

func (c *Connector) chatState(jid types.JID, set func(*Chat)) {
	if chatSkipped(jid) {
		return
	}
	ch := Chat{ID: c.canon(jid).String(), Kind: chatKind(jid)}
	set(&ch)
	c.out.Event("chat.upsert", map[string]any{"chat": ch})
}

// message: one live message (history goes through history()).
func (c *Connector) message(evt *events.Message, live bool) {
	out := Convert(evt, c.canon)
	ctx, cancel := context.WithTimeout(context.Background(), commandTimeout)
	defer cancel()
	m := out.Message
	switch out.Kind {
	case content:
		body := ""
		if m.Text != nil {
			body = m.Text.Plain
		}
		c.noteUsers(evt)
		c.local.Remember(ctx, m.Chat, m.ID, evt.Info.Chat.String(), evt.Info.Sender.ToNonAD().String(), m.FromMe, m.Timestamp, body, out.MediaProto)
		c.out.Event("message.new", map[string]any{"live": live, "message": m})
	case reaction:
		c.out.Event("reaction", map[string]any{
			"chat": m.Chat, "message": out.Target, "sender": m.Sender, "from_me": m.FromMe, "emoji": out.Emoji, "timestamp": m.Timestamp,
		})
	case edit:
		c.local.Edited(ctx, m.Chat, out.Target, out.Text.Plain)
		c.out.Event("message.edit", map[string]any{"chat": m.Chat, "id": out.Target, "text": out.Text, "timestamp": m.Timestamp})
	case revoke:
		c.out.Event("message.delete", map[string]any{"chat": m.Chat, "id": out.Target})
	}
}

// noteUsers: the other person of a direct chat and the sender of a message are announced once.
func (c *Connector) noteUsers(evt *events.Message) {
	if evt.Info.Chat.Server != types.GroupServer {
		c.ensureContact(evt.Info.Chat, "")
	}
	if !evt.Info.IsFromMe {
		c.ensureContact(evt.Info.Sender, evt.Info.PushName)
	}
}

func (c *Connector) receipt(evt *events.Receipt) {
	if chatSkipped(evt.Chat) || len(evt.MessageIDs) == 0 {
		return
	}
	fields := map[string]any{
		"chat": c.canon(evt.Chat).String(), "messages": evt.MessageIDs, "timestamp": evt.Timestamp.UnixMilli(),
	}
	switch evt.Type {
	case types.ReceiptTypeDelivered:
		fields["kind"] = "delivered"
	case types.ReceiptTypeRead, types.ReceiptTypePlayed:
		fields["kind"] = "read"
	case types.ReceiptTypeReadSelf, types.ReceiptTypePlayedSelf:
		// Read on the phone or another linked device: the chat is read here too.
		fields["kind"] = "read"
		fields["from_me"] = true
	default:
		return
	}
	if !evt.IsFromMe {
		fields["sender"] = c.canon(evt.Sender).String()
	}
	c.out.Event("receipt", fields)
}

func (c *Connector) history(evt *events.HistorySync) {
	data := evt.Data
	if data == nil {
		return
	}
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Minute)
	defer cancel()
	for _, p := range data.GetPushnames() {
		if jid, err := types.ParseJID(p.GetID()); err == nil {
			c.pushName(jid, p.GetPushname())
		}
	}
	cli := c.cli()
	var batch []Message
	flush := func(last bool) {
		if len(batch) == 0 {
			return
		}
		fields := map[string]any{"messages": batch}
		if last {
			fields["progress"] = map[string]any{"done": data.GetProgress(), "total": 100}
		}
		c.out.Event("history.batch", fields)
		batch = nil
	}
	for _, conv := range data.GetConversations() {
		chatJID, err := types.ParseJID(conv.GetID())
		if err != nil || chatSkipped(chatJID) {
			continue
		}
		ch := Chat{ID: c.canon(chatJID).String(), Kind: chatKind(chatJID), Title: conv.GetName()}
		unread := int64(conv.GetUnreadCount())
		archived := conv.GetArchived()
		pinned := conv.GetPinned() > 0
		ch.Unread, ch.Archived, ch.Pinned = &unread, &archived, &pinned
		if end := conv.GetMuteEndTime(); end != 0 {
			until := int64(end)
			if until > 0 && until < 1e12 {
				until *= 1000 // seconds in history, milliseconds in the protocol
			}
			ch.MutedUntil = &until
		}
		c.out.Event("chat.upsert", map[string]any{"chat": ch})
		if chatJID.Server != types.GroupServer {
			c.ensureContact(chatJID, "")
		}
		for _, hm := range conv.GetMessages() {
			parsed, err := cli.ParseWebMessage(chatJID, hm.GetMessage())
			if err != nil {
				continue
			}
			out := Convert(parsed, c.canon)
			switch out.Kind {
			case content:
				body := ""
				if out.Message.Text != nil {
					body = out.Message.Text.Plain
				}
				c.noteUsers(parsed)
				c.local.Remember(ctx, out.Message.Chat, out.Message.ID, parsed.Info.Chat.String(), parsed.Info.Sender.ToNonAD().String(),
					out.Message.FromMe, out.Message.Timestamp, body, out.MediaProto)
				batch = append(batch, out.Message)
				if len(batch) >= historyChunk {
					flush(false)
				}
			case edit:
				// The edited text replaces the original in its batch, or arrives as an edit if it was already sent.
				replaced := false
				for i := range batch {
					if batch[i].ID == out.Target {
						batch[i].Text = out.Text
						replaced = true
					}
				}
				if !replaced {
					c.out.Event("message.edit", map[string]any{"chat": out.Message.Chat, "id": out.Target, "text": out.Text})
				}
			}
		}
	}
	flush(true)
}

// A push name (the name people give themselves) only when the address book has nothing better.
func (c *Connector) pushName(jid types.JID, name string) {
	if name == "" || jid.Server == types.GroupServer {
		return
	}
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	if info, err := c.cli().Store.Contacts.GetContact(ctx, jid); err == nil && info.Found && (info.FullName != "" || info.FirstName != "") {
		return
	}
	c.contact(jid, name)
}

// ensureContact: the first time a user shows up, tell the daemon their phone number, and their push name when the
// address book has no name for them; a stranger then reads "+33…" or "~Name" instead of an internal id.
func (c *Connector) ensureContact(jid types.JID, pushName string) {
	id := c.canon(jid)
	if id.Server != types.DefaultUserServer && id.Server != types.HiddenUserServer {
		return
	}
	if _, seen := c.announced.LoadOrStore(id.String(), true); seen {
		return
	}
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	c.local.Seen(ctx, jid.ToNonAD().String())
	cancel()
	if phone := phoneOf(id); phone != "" {
		c.out.Event("contact.upsert", map[string]any{"contact": Contact{ID: id.String(), Phone: phone}})
	}
	c.pushName(jid, pushName)
}

func (c *Connector) contact(jid types.JID, name string) {
	id := c.canon(jid)
	c.out.Event("contact.upsert", map[string]any{"contact": Contact{ID: id.String(), Name: name, Phone: phoneOf(id)}})
}

// After connecting: the address book and the groups.
func (c *Connector) syncDirectory() {
	ctx, cancel := context.WithTimeout(context.Background(), 2*time.Minute)
	defer cancel()
	cli := c.cli()
	c.local.Prune(ctx)
	for _, raw := range c.local.Users(ctx) {
		if jid, err := types.ParseJID(raw); err == nil {
			c.ensureContact(jid, "")
		}
	}
	if contacts, err := cli.Store.Contacts.GetAllContacts(ctx); err == nil {
		for jid, info := range contacts {
			name := info.FullName
			for _, n := range []string{info.FirstName, info.BusinessName, info.PushName} {
				if name == "" {
					name = n
				}
			}
			id := c.canon(jid)
			c.out.Event("contact.upsert", map[string]any{"contact": Contact{ID: id.String(), Name: name, Phone: phoneOf(id)}})
		}
	}
	if groups, err := cli.GetJoinedGroups(ctx); err == nil {
		for _, g := range groups {
			c.out.Event("chat.upsert", map[string]any{"chat": Chat{ID: g.JID.String(), Kind: "group", Title: g.Name}})
		}
	} else {
		fmt.Fprintf(os.Stderr, "fili-im-whatsapp: groups: %v\n", err)
	}
}

// The session is gone: start over with a new device, waiting for pairing.
func (c *Connector) forget(detail string) {
	old := c.cli()
	old.Disconnect()
	if old.Store.ID != nil {
		_ = old.Store.Delete(context.Background())
	}
	c.setClient(c.container.NewDevice())
	c.out.State("needs_pairing", detail, nil)
}

// ---------------------------------------------------------------------------------------------------------------
// Commands

type cmdError struct {
	code, message string
}

func (e *cmdError) Error() string { return e.message }

func fail(code, format string, args ...any) error {
	return &cmdError{code, fmt.Sprintf(format, args...)}
}

// Run executes one command and returns its result; errors carry a protocol code.
func (c *Connector) Run(cmd *Command) (any, error) {
	ctx, cancel := context.WithTimeout(context.Background(), commandTimeout)
	defer cancel()
	switch cmd.Cmd {
	case "pair":
		return nil, c.pair(cmd.Method, cmd.Phone)
	case "logout":
		return nil, c.logout(ctx)
	case "shutdown", "chat.archive", "chat.mute", "send", "edit", "delete", "react", "mark_read", "typing", "fetch_media", "avatar", "presence", "stream":
	default:
		return nil, fail("unsupported", "unknown command %q", cmd.Cmd)
	}
	if cmd.Cmd == "shutdown" {
		return nil, nil
	}
	cli := c.cli()
	if cli.Store.ID == nil {
		return nil, fail("not_connected", "not paired")
	}
	if cmd.Cmd != "fetch_media" && cmd.Cmd != "stream" && !cli.IsConnected() {
		return nil, fail("not_connected", "not connected to WhatsApp")
	}
	if cmd.Cmd == "presence" {
		state := types.PresenceUnavailable
		if cmd.State == "available" {
			state = types.PresenceAvailable
		}
		if err := cli.SendPresence(ctx, state); err != nil {
			return nil, sendError(err)
		}
		return nil, nil
	}
	chat, err := types.ParseJID(cmd.Chat)
	if err != nil || cmd.Chat == "" {
		return nil, fail("invalid", "bad chat id")
	}
	if chat.Server != types.DefaultUserServer && chat.Server != types.GroupServer && chat.Server != types.HiddenUserServer {
		return nil, fail("not_found", "no such chat")
	}
	switch cmd.Cmd {
	case "send":
		return c.send(ctx, cli, chat, cmd)
	case "edit":
		if cmd.Text == nil || strings.TrimSpace(cmd.Text.Plain) == "" {
			return nil, fail("invalid", "empty text")
		}
		edited := cli.BuildEdit(chat, cmd.Message, &waE2E.Message{Conversation: proto.String(FormatMarkup(*cmd.Text))})
		if _, err := cli.SendMessage(ctx, chat, edited); err != nil {
			return nil, sendError(err)
		}
		c.local.Edited(ctx, chat.String(), cmd.Message, cmd.Text.Plain)
		return nil, nil
	case "delete":
		if !cmd.Everyone {
			return nil, fail("unsupported", "deleting only for me is not available yet")
		}
		sender := types.EmptyJID
		if k, ok := c.local.Known(ctx, chat.String(), cmd.Message); ok && !k.FromMe {
			sender, _ = types.ParseJID(k.RawSender) // a group admin deleting someone else's message
		}
		if _, err := cli.SendMessage(ctx, chat, cli.BuildRevoke(chat, sender, cmd.Message)); err != nil {
			return nil, sendError(err)
		}
		return nil, nil
	case "react":
		target, sender := chat, types.EmptyJID
		if k, ok := c.local.Known(ctx, chat.String(), cmd.Message); ok {
			if raw, err := types.ParseJID(k.RawChat); err == nil {
				target = raw
			}
			if !k.FromMe {
				sender, _ = types.ParseJID(k.RawSender)
			}
		} else if chat.Server == types.GroupServer {
			return nil, fail("not_found", "this message is too old to react to from here")
		} else {
			sender = chat // a direct chat: the other person sent it
		}
		if _, err := cli.SendMessage(ctx, target, cli.BuildReaction(target, sender, cmd.Message, cmd.Emoji)); err != nil {
			return nil, sendError(err)
		}
		return nil, nil
	case "mark_read":
		return nil, c.markRead(ctx, cli, chat, cmd)
	case "typing":
		state := types.ChatPresencePaused
		if cmd.Active {
			state = types.ChatPresenceComposing
		}
		return nil, cli.SendChatPresence(ctx, chat, state, types.ChatPresenceMediaText)
	case "chat.mute":
		var patch appstate.PatchInfo
		switch {
		case cmd.Until == 0:
			patch = appstate.BuildMute(chat, false, 0)
		case cmd.Until < 0:
			patch = appstate.BuildMute(chat, true, 0)
		default:
			until := cmd.Until
			patch = appstate.BuildMuteAbs(chat, true, &until)
		}
		return nil, cli.SendAppState(ctx, patch)
	case "chat.archive":
		id, k, ok := c.local.Latest(ctx, chat.String())
		if !ok {
			return nil, fail("not_found", "no recent message in this chat to archive it")
		}
		key := &waCommon.MessageKey{RemoteJID: proto.String(k.RawChat), FromMe: proto.Bool(k.FromMe), ID: proto.String(id)}
		if chat.Server == types.GroupServer && !k.FromMe {
			key.Participant = proto.String(k.RawSender)
		}
		return nil, cli.SendAppState(ctx, appstate.BuildArchive(chat, cmd.Archived, time.UnixMilli(k.Timestamp), key))
	case "fetch_media":
		return c.fetchMedia(ctx, cli, chat, cmd)
	case "avatar":
		return c.avatar(ctx, cli, chat, cmd)
	case "stream":
		return c.stream(ctx, chat.String(), cmd)
	}
	return nil, fail("unsupported", "unknown command %q", cmd.Cmd)
}

func sendError(err error) error {
	switch {
	case errors.Is(err, whatsmeow.ErrNotConnected), errors.Is(err, whatsmeow.ErrNotLoggedIn):
		return fail("not_connected", "not connected to WhatsApp")
	case errors.Is(err, context.DeadlineExceeded):
		return fail("failed", "WhatsApp did not answer in time")
	}
	return fail("failed", "%v", err)
}

func (c *Connector) send(ctx context.Context, cli *whatsmeow.Client, chat types.JID, cmd *Command) (any, error) {
	hasMedia := len(cmd.Media) > 0 && string(cmd.Media) != "null"
	if !hasMedia && (cmd.Text == nil || strings.TrimSpace(cmd.Text.Plain) == "") {
		return nil, fail("invalid", "empty message")
	}
	if cmd.Text == nil {
		cmd.Text = &Text{}
	}
	// Idempotent on client_id: the same id goes out again, and WhatsApp drops the duplicate.
	id, seen := "", false
	if cmd.ClientID != "" {
		id, seen = c.local.SentID(ctx, cmd.ClientID)
	}
	if !seen {
		id = cli.GenerateMessageID()
		if cmd.ClientID != "" {
			if err := c.local.RememberSent(ctx, cmd.ClientID, id); err != nil {
				return nil, fail("failed", "cannot record the message: %v", err)
			}
		}
	}
	text := FormatMarkup(*cmd.Text)
	msg := &waE2E.Message{Conversation: proto.String(text)}
	var ctxInfo *waE2E.ContextInfo
	if cmd.ReplyTo != "" {
		ctxInfo = &waE2E.ContextInfo{StanzaID: proto.String(cmd.ReplyTo)}
		if k, ok := c.local.Known(ctx, chat.String(), cmd.ReplyTo); ok {
			participant := k.RawSender
			if k.FromMe {
				participant = cli.Store.ID.ToNonAD().String()
			}
			ctxInfo.Participant = proto.String(participant)
			ctxInfo.QuotedMessage = &waE2E.Message{Conversation: proto.String(k.Body)}
		}
		msg = &waE2E.Message{ExtendedTextMessage: &waE2E.ExtendedTextMessage{Text: proto.String(text), ContextInfo: ctxInfo}}
	}
	if hasMedia {
		built, err := c.mediaMessage(ctx, cli, cmd.Media, text, ctxInfo)
		if err != nil {
			return nil, err
		}
		msg = built
	}
	resp, err := cli.SendMessage(ctx, chat, msg, whatsmeow.SendRequestExtra{ID: id})
	if err != nil {
		return nil, sendError(err)
	}
	ts := resp.Timestamp.UnixMilli()
	if resp.Timestamp.IsZero() {
		ts = time.Now().UnixMilli()
	}
	c.local.Remember(ctx, chat.String(), resp.ID, chat.String(), cli.Store.ID.ToNonAD().String(), true, ts, cmd.Text.Plain, nil)
	return map[string]any{"id": resp.ID, "timestamp": ts}, nil
}

func (c *Connector) markRead(ctx context.Context, cli *whatsmeow.Client, chat types.JID, cmd *Command) error {
	if len(cmd.Messages) == 0 {
		return nil
	}
	// Receipts go to the chat and sender as WhatsApp addressed the messages (a LID, possibly).
	target, sender := chat, types.EmptyJID
	if k, ok := c.local.Known(ctx, chat.String(), cmd.Messages[len(cmd.Messages)-1]); ok {
		if raw, err := types.ParseJID(k.RawChat); err == nil {
			target = raw
		}
		if chat.Server == types.GroupServer {
			sender, _ = types.ParseJID(k.RawSender)
		}
	} else if chat.Server == types.GroupServer && cmd.Sender != "" {
		sender, _ = types.ParseJID(cmd.Sender)
	}
	return cli.MarkRead(ctx, cmd.Messages, time.Now(), target, sender)
}

// avatar saves the small profile picture of a contact or a group to cmd.Dest. Hidden or missing pictures are not_found.
func (c *Connector) avatar(ctx context.Context, cli *whatsmeow.Client, chat types.JID, cmd *Command) (any, error) {
	if cmd.Dest == "" {
		return nil, fail("invalid", "missing dest")
	}
	info, err := cli.GetProfilePictureInfo(ctx, chat, &whatsmeow.GetProfilePictureParams{Preview: true})
	if errors.Is(err, whatsmeow.ErrProfilePictureNotSet) || errors.Is(err, whatsmeow.ErrProfilePictureUnauthorized) || (err == nil && info == nil) {
		return nil, fail("not_found", "no profile picture")
	}
	if err != nil {
		return nil, sendError(err)
	}
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, info.URL, nil)
	if err != nil {
		return nil, fail("failed", "bad picture address")
	}
	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		return nil, fail("failed", "picture download failed")
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		return nil, fail("failed", "picture download failed with status code %d", resp.StatusCode)
	}
	data, err := io.ReadAll(io.LimitReader(resp.Body, 4<<20))
	if err != nil {
		return nil, fail("failed", "picture download failed")
	}
	if err := os.WriteFile(cmd.Dest, data, 0o600); err != nil {
		return nil, fail("failed", "cannot write the picture: %v", err)
	}
	return nil, nil
}

func (c *Connector) fetchMedia(ctx context.Context, cli *whatsmeow.Client, chat types.JID, cmd *Command) (any, error) {
	if cmd.Dest == "" {
		return nil, fail("invalid", "missing dest")
	}
	ref := cmd.Ref
	if ref == "" {
		ref = cmd.Message
	}
	m, err := c.local.Media(ctx, chat.String(), ref)
	if err != nil {
		return nil, fail("not_found", "no media for this message")
	}
	var part whatsmeow.DownloadableMessage
	switch {
	case m.GetImageMessage() != nil:
		part = m.GetImageMessage()
	case m.GetVideoMessage() != nil:
		part = m.GetVideoMessage()
	case m.GetAudioMessage() != nil:
		part = m.GetAudioMessage()
	case m.GetDocumentMessage() != nil:
		part = m.GetDocumentMessage()
	case m.GetStickerMessage() != nil:
		part = m.GetStickerMessage()
	default:
		return nil, fail("not_found", "no media for this message")
	}
	dl, cancel := context.WithTimeout(context.Background(), 5*time.Minute)
	defer cancel()
	data, err := cli.Download(dl, part)
	if err != nil {
		if errors.Is(err, whatsmeow.ErrMediaDownloadFailedWith404) || errors.Is(err, whatsmeow.ErrMediaDownloadFailedWith410) {
			return nil, fail("not_found", "the file is no longer on WhatsApp's servers")
		}
		return nil, fail("failed", "download failed: %v", err)
	}
	if err := os.WriteFile(cmd.Dest, data, 0o600); err != nil {
		return nil, fail("failed", "cannot write the file: %v", err)
	}
	return map[string]any{"path": cmd.Dest}, nil
}

// pair starts a QR or phone-code pairing; the codes arrive as `pairing` events until the phone confirms.
func (c *Connector) pair(method, phone string) error {
	cli := c.cli()
	if cli.Store.ID != nil {
		return fail("invalid", "already paired")
	}
	if method != "qr" && method != "code" {
		return fail("unsupported", "pairing by %q", method)
	}
	phone = strings.TrimLeft(strings.Map(func(r rune) rune {
		if r >= '0' && r <= '9' {
			return r
		}
		return -1
	}, phone), "0")
	if method == "code" && len(phone) < 6 {
		return fail("invalid", "give the phone number in international format (+33…)")
	}
	c.mu.Lock()
	if c.pairing != nil {
		c.pairing()
	}
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Minute)
	c.pairing = cancel
	c.mu.Unlock()
	cli.Disconnect()
	qr, err := cli.GetQRChannel(ctx)
	if err != nil {
		cancel()
		return fail("failed", "cannot start pairing: %v", err)
	}
	if err := cli.Connect(); err != nil {
		cancel()
		return fail("failed", "cannot connect to WhatsApp: %v", err)
	}
	go func() {
		defer cancel()
		codeSent := false
		for item := range qr {
			switch item.Event {
			case "code":
				if method == "qr" {
					c.out.Event("pairing", map[string]any{"kind": "qr", "data": item.Code, "expires_s": int(item.Timeout.Seconds())})
				} else if !codeSent {
					code, err := cli.PairPhone(ctx, phone, true, whatsmeow.PairClientChrome, "Chrome (Linux)")
					if err != nil {
						c.out.State("needs_pairing", "Pairing by code failed: "+err.Error(), nil)
						cli.Disconnect()
						return
					}
					codeSent = true
					c.out.Event("pairing", map[string]any{"kind": "code", "data": code, "expires_s": 180})
				}
			case "success":
				// events.Connected follows.
			case "timeout":
				c.out.State("needs_pairing", "The pairing code expired; try again", nil)
				cli.Disconnect()
			case "err-client-outdated":
				c.out.State("error", "WhatsApp rejects this connector as outdated; update fili-im", nil)
			case "err-scanned-without-multidevice":
				c.out.State("needs_pairing", "Update WhatsApp on the phone, then scan again", nil)
			default:
				if item.Error != nil {
					c.out.State("needs_pairing", "Pairing failed: "+item.Error.Error(), nil)
				}
			}
		}
	}()
	return nil
}

func (c *Connector) logout(ctx context.Context) error {
	cli := c.cli()
	if cli.Store.ID == nil {
		c.out.State("logged_out", "", nil)
		return nil
	}
	if cli.IsConnected() {
		if err := cli.Logout(ctx); err != nil {
			fmt.Fprintf(os.Stderr, "fili-im-whatsapp: logout: %v\n", err)
		}
	}
	cli.Disconnect()
	if cli.Store.ID != nil {
		_ = cli.Store.Delete(ctx)
	}
	c.setClient(c.container.NewDevice())
	c.out.State("logged_out", "", nil)
	return nil
}

// mediaMessage uploads the file of a `send` command and wraps it as the WhatsApp message of its kind; the text is the
// caption.
func (c *Connector) mediaMessage(ctx context.Context, cli *whatsmeow.Client, raw json.RawMessage, caption string, ctxInfo *waE2E.ContextInfo) (*waE2E.Message, error) {
	var in struct {
		Path string `json:"path"`
		Kind string `json:"kind"`
		Mime string `json:"mime"`
		Name string `json:"name"`
	}
	if err := json.Unmarshal(raw, &in); err != nil || in.Path == "" {
		return nil, fail("invalid", "missing media path")
	}
	data, err := os.ReadFile(in.Path)
	if err != nil {
		return nil, fail("failed", "cannot read the file: %v", err)
	}
	mime := in.Mime
	if mime == "" {
		mime = http.DetectContentType(data)
	}
	name := in.Name
	if name == "" {
		name = filepath.Base(in.Path)
	}
	kind := whatsmeow.MediaDocument
	switch in.Kind {
	case "image":
		kind = whatsmeow.MediaImage
	case "video":
		kind = whatsmeow.MediaVideo
	}
	up, err := cli.Upload(ctx, data, kind)
	if err != nil {
		return nil, sendError(err)
	}
	var capt *string
	if caption != "" {
		capt = proto.String(caption)
	}
	switch in.Kind {
	case "image":
		return &waE2E.Message{ImageMessage: &waE2E.ImageMessage{
			URL: &up.URL, DirectPath: &up.DirectPath, MediaKey: up.MediaKey, Mimetype: &mime,
			FileEncSHA256: up.FileEncSHA256, FileSHA256: up.FileSHA256, FileLength: &up.FileLength,
			Caption: capt, ContextInfo: ctxInfo,
		}}, nil
	case "video":
		return &waE2E.Message{VideoMessage: &waE2E.VideoMessage{
			URL: &up.URL, DirectPath: &up.DirectPath, MediaKey: up.MediaKey, Mimetype: &mime,
			FileEncSHA256: up.FileEncSHA256, FileSHA256: up.FileSHA256, FileLength: &up.FileLength,
			Caption: capt, ContextInfo: ctxInfo,
		}}, nil
	}
	return &waE2E.Message{DocumentMessage: &waE2E.DocumentMessage{
		URL: &up.URL, DirectPath: &up.DirectPath, MediaKey: up.MediaKey, Mimetype: &mime,
		FileEncSHA256: up.FileEncSHA256, FileSHA256: up.FileSHA256, FileLength: &up.FileLength,
		FileName: proto.String(name), Title: proto.String(name), Caption: capt, ContextInfo: ctxInfo,
	}}, nil
}
