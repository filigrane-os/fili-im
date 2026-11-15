package main

import (
	"reflect"
	"testing"
	"time"

	"go.mau.fi/whatsmeow/proto/waCommon"
	"go.mau.fi/whatsmeow/proto/waE2E"
	"go.mau.fi/whatsmeow/types"
	"go.mau.fi/whatsmeow/types/events"
	"google.golang.org/protobuf/proto"
)

var paul = types.NewJID("33611111111", types.DefaultUserServer)
var paulLID = types.NewJID("123456789", types.HiddenUserServer)
var family = types.NewJID("120363000000000001", types.GroupServer)

// The LID of Paul maps to his phone number; everything else stays, without device.
func canon(j types.JID) types.JID {
	if j.ToNonAD() == paulLID {
		return paul
	}
	return j.ToNonAD()
}

func msg(chat, sender types.JID, m *waE2E.Message) *events.Message {
	return &events.Message{
		Info: types.MessageInfo{
			MessageSource: types.MessageSource{Chat: chat, Sender: sender, IsGroup: chat.Server == types.GroupServer},
			ID:            "3EB0AAA",
			Timestamp:     time.UnixMilli(1763200000000),
		},
		Message: m,
	}
}

func TestTextAndMarkup(t *testing.T) {
	out := Convert(msg(paul, paul, &waE2E.Message{Conversation: proto.String("On *mange* où ?")}), canon)
	if out.Kind != content || out.Message.Kind != "text" {
		t.Fatalf("got %+v", out)
	}
	want := Text{Plain: "On mange où ?", Spans: []Span{{Start: 3, End: 8, Style: "bold"}}}
	if !reflect.DeepEqual(*out.Message.Text, want) {
		t.Fatalf("text %+v", *out.Message.Text)
	}
	if out.Message.Chat != "33611111111@s.whatsapp.net" || out.Message.Sender != "33611111111@s.whatsapp.net" {
		t.Fatalf("ids %+v", out.Message)
	}
	if out.Message.Timestamp != 1763200000000 {
		t.Fatalf("timestamp %d", out.Message.Timestamp)
	}
}

func TestLIDSenderInAGroupBecomesThePhoneNumber(t *testing.T) {
	device := types.JID{User: "123456789", Server: types.HiddenUserServer, Device: 3}
	out := Convert(msg(family, device, &waE2E.Message{Conversation: proto.String("hi")}), canon)
	if out.Message.Sender != "33611111111@s.whatsapp.net" || out.Message.Chat != family.String() {
		t.Fatalf("got %+v", out.Message)
	}
	e := msg(family, types.NewJID("999", types.HiddenUserServer), &waE2E.Message{Conversation: proto.String("hi")})
	e.Info.SenderAlt = types.NewJID("33622222222", types.DefaultUserServer)
	if out := Convert(e, canon); out.Message.Sender != "33622222222@s.whatsapp.net" {
		t.Fatalf("SenderAlt not used: %+v", out.Message)
	}
}

func TestFromMeHasNoSender(t *testing.T) {
	e := msg(paul, paul, &waE2E.Message{Conversation: proto.String("mine")})
	e.Info.IsFromMe = true
	out := Convert(e, canon)
	if !out.Message.FromMe || out.Message.Sender != "" {
		t.Fatalf("got %+v", out.Message)
	}
}

func TestReplyMediaAndKinds(t *testing.T) {
	reply := &waE2E.Message{ExtendedTextMessage: &waE2E.ExtendedTextMessage{
		Text: proto.String("oui"), ContextInfo: &waE2E.ContextInfo{StanzaID: proto.String("3EA9")},
	}}
	if out := Convert(msg(paul, paul, reply), canon); out.Message.ReplyTo != "3EA9" {
		t.Fatalf("reply %+v", out.Message)
	}
	image := &waE2E.Message{ImageMessage: &waE2E.ImageMessage{Caption: proto.String("plage"), Mimetype: proto.String("image/jpeg"), FileLength: proto.Uint64(1200)}}
	out := Convert(msg(paul, paul, image), canon)
	if out.Message.Kind != "image" || out.Message.Text.Plain != "plage" || out.Message.Media.Ref != "3EB0AAA" || out.MediaProto.GetImageMessage() == nil {
		t.Fatalf("image %+v", out)
	}
	voice := &waE2E.Message{AudioMessage: &waE2E.AudioMessage{PTT: proto.Bool(true)}}
	if out := Convert(msg(paul, paul, voice), canon); out.Message.Kind != "voice" || out.Message.Text != nil {
		t.Fatalf("voice %+v", out.Message)
	}
	doc := &waE2E.Message{DocumentMessage: &waE2E.DocumentMessage{FileName: proto.String("devis.pdf")}}
	if out := Convert(msg(paul, paul, doc), canon); out.Message.Kind != "file" || out.Message.Media.Name != "devis.pdf" {
		t.Fatalf("doc %+v", out.Message)
	}
	poll := &waE2E.Message{PollCreationMessageV3: &waE2E.PollCreationMessage{
		Name: proto.String("Dimanche ?"), Options: []*waE2E.PollCreationMessage_Option{{OptionName: proto.String("Oui")}},
	}}
	if out := Convert(msg(family, paul, poll), canon); out.Message.Kind != "poll" || out.Message.Text.Plain != "Dimanche ?\n• Oui" {
		t.Fatalf("poll %+v", out.Message)
	}
	odd := &waE2E.Message{EventMessage: &waE2E.EventMessage{Name: proto.String("x")}}
	if out := Convert(msg(paul, paul, odd), canon); out.Kind != content || out.Message.Kind != "unsupported" {
		t.Fatalf("unknown kinds are kept as unsupported: %+v", out)
	}
}

func TestReactionsEditsRevokesAndNoise(t *testing.T) {
	r := &waE2E.Message{ReactionMessage: &waE2E.ReactionMessage{Key: &waCommon.MessageKey{ID: proto.String("T1")}, Text: proto.String("👍")}}
	if out := Convert(msg(paul, paul, r), canon); out.Kind != reaction || out.Target != "T1" || out.Emoji != "👍" {
		t.Fatalf("reaction %+v", out)
	}
	ed := &waE2E.Message{ProtocolMessage: &waE2E.ProtocolMessage{
		Type: waE2E.ProtocolMessage_MESSAGE_EDIT.Enum(), Key: &waCommon.MessageKey{ID: proto.String("T2")},
		EditedMessage: &waE2E.Message{Conversation: proto.String("corrigé")},
	}}
	if out := Convert(msg(paul, paul, ed), canon); out.Kind != edit || out.Target != "T2" || out.Text.Plain != "corrigé" {
		t.Fatalf("edit %+v", out)
	}
	rv := &waE2E.Message{ProtocolMessage: &waE2E.ProtocolMessage{Type: waE2E.ProtocolMessage_REVOKE.Enum(), Key: &waCommon.MessageKey{ID: proto.String("T3")}}}
	if out := Convert(msg(paul, paul, rv), canon); out.Kind != revoke || out.Target != "T3" {
		t.Fatalf("revoke %+v", out)
	}
	keys := &waE2E.Message{SenderKeyDistributionMessage: &waE2E.SenderKeyDistributionMessage{}}
	if out := Convert(msg(family, paul, keys), canon); out.Kind != skip {
		t.Fatalf("key distribution alone is noise: %+v", out)
	}
	other := &waE2E.Message{ProtocolMessage: &waE2E.ProtocolMessage{Type: waE2E.ProtocolMessage_HISTORY_SYNC_NOTIFICATION.Enum()}}
	if out := Convert(msg(paul, paul, other), canon); out.Kind != skip {
		t.Fatalf("protocol bookkeeping is skipped: %+v", out)
	}
	status := msg(types.StatusBroadcastJID, paul, &waE2E.Message{Conversation: proto.String("story")})
	if out := Convert(status, canon); out.Kind != skip {
		t.Fatalf("stories are skipped: %+v", out)
	}
}
