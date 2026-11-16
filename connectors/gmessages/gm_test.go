package main

import (
	"strings"
	"testing"

	"go.mau.fi/mautrix-gmessages/pkg/libgm/gmproto"
)

func message(status gmproto.MessageStatusType, parts ...*gmproto.MessageInfo) *gmproto.Message {
	return &gmproto.Message{
		MessageID: "m1", ConversationID: "c1", ParticipantID: "p2", Timestamp: 1_700_000_000_000_000,
		MessageStatus: &gmproto.MessageStatus{Status: status}, MessageInfo: parts,
	}
}

func text(s string) *gmproto.MessageInfo {
	return &gmproto.MessageInfo{Data: &gmproto.MessageInfo_MessageContent{MessageContent: &gmproto.MessageContent{Content: s}}}
}

func TestConvertDirectionAndTime(t *testing.T) {
	c := &Connector{}
	in, ok := c.convert(message(gmproto.MessageStatusType_INCOMING_COMPLETE, text("hi")))
	if !ok || in.FromMe || in.Sender != "p2" || in.Timestamp != 1_700_000_000_000 || in.Text.Plain != "hi" {
		t.Fatalf("incoming: %+v", in)
	}
	out, ok := c.convert(message(gmproto.MessageStatusType_OUTGOING_COMPLETE, text("yo")))
	if !ok || !out.FromMe || out.Sender != "" {
		t.Fatalf("outgoing: %+v", out)
	}
}

func TestConvertSkipsTombstonesAndEmpty(t *testing.T) {
	c := &Connector{}
	if _, ok := c.convert(message(gmproto.MessageStatusType_TOMBSTONE_PROTOCOL_SWITCH_TO_RCS, text("x"))); ok {
		t.Fatal("a tombstone is not a message")
	}
	if _, ok := c.convert(message(gmproto.MessageStatusType_INCOMING_COMPLETE)); ok {
		t.Fatal("an empty message is not shown")
	}
}

func TestConvertMediaRef(t *testing.T) {
	c := &Connector{}
	media := &gmproto.MessageInfo{Data: &gmproto.MessageInfo_MediaContent{MediaContent: &gmproto.MediaContent{
		MediaID: "abc", MimeType: "image/jpeg", MediaName: "a.jpg", Size: 10, DecryptionKey: []byte{1, 2, 3},
	}}}
	m, ok := c.convert(message(gmproto.MessageStatusType_INCOMING_COMPLETE, media, text("caption")))
	if !ok || m.Kind != "image" || m.Media == nil || !strings.HasPrefix(m.Media.Ref, "abc|") || m.Text.Plain != "caption" {
		t.Fatalf("media: %+v", m)
	}
}
