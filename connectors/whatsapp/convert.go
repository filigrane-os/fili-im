// WhatsApp messages → protocol events. Pure functions over whatsmeow's types, so they are tested without a network.
package main

import (
	"fmt"
	"strings"

	"go.mau.fi/whatsmeow/proto/waE2E"
	"go.mau.fi/whatsmeow/types"
	"go.mau.fi/whatsmeow/types/events"
)

// Canon gives the one stable id the daemon sees for a user or a chat: the phone-number JID when WhatsApp's LID can be
// mapped to it, without device part.
type Canon func(types.JID) types.JID

type outcomeKind int

const (
	skip outcomeKind = iota
	content
	reaction
	edit
	revoke
)

type Outcome struct {
	Kind    outcomeKind
	Message Message // content: the message; others: Chat, Sender, FromMe, Timestamp of the event
	Target  string  // reaction, edit, revoke: the message acted on
	Emoji   string
	Text    *Text
	// The media part, kept so fetch_media can download it later.
	MediaProto *waE2E.Message
}

// chatSkipped: stories, newsletters and broadcast lists are not part of Conversations (yet).
func chatSkipped(chat types.JID) bool {
	return chat.Server == types.BroadcastServer || chat.Server == types.NewsletterServer
}

func chatKind(chat types.JID) string {
	if chat.Server == types.GroupServer {
		return "group"
	}
	return "direct"
}

func contextOf(m *waE2E.Message) *waE2E.ContextInfo {
	switch {
	case m.GetExtendedTextMessage() != nil:
		return m.GetExtendedTextMessage().GetContextInfo()
	case m.GetImageMessage() != nil:
		return m.GetImageMessage().GetContextInfo()
	case m.GetVideoMessage() != nil:
		return m.GetVideoMessage().GetContextInfo()
	case m.GetAudioMessage() != nil:
		return m.GetAudioMessage().GetContextInfo()
	case m.GetDocumentMessage() != nil:
		return m.GetDocumentMessage().GetContextInfo()
	case m.GetStickerMessage() != nil:
		return m.GetStickerMessage().GetContextInfo()
	case m.GetLocationMessage() != nil:
		return m.GetLocationMessage().GetContextInfo()
	case m.GetContactMessage() != nil:
		return m.GetContactMessage().GetContextInfo()
	}
	return nil
}

// noise: message types that carry nothing to show (key distribution, protocol bookkeeping, poll votes…).
func noise(m *waE2E.Message) bool {
	return m.GetPollUpdateMessage() != nil || m.GetKeepInChatMessage() != nil || m.GetPinInChatMessage() != nil ||
		m.GetEncReactionMessage() != nil || m.GetSecretEncryptedMessage() != nil ||
		(m.GetSenderKeyDistributionMessage() != nil && m.GetConversation() == "" && m.GetExtendedTextMessage() == nil &&
			m.GetImageMessage() == nil && m.GetVideoMessage() == nil && m.GetAudioMessage() == nil &&
			m.GetDocumentMessage() == nil && m.GetStickerMessage() == nil)
}

// body: kind, text, media part of a content message; ok false when the message is not content.
func body(m *waE2E.Message) (kind string, text string, media *Media, part *waE2E.Message, ok bool) {
	switch {
	case m.Conversation != nil:
		return "text", m.GetConversation(), nil, nil, true
	case m.GetExtendedTextMessage() != nil:
		return "text", m.GetExtendedTextMessage().GetText(), nil, nil, true
	case m.GetImageMessage() != nil:
		im := m.GetImageMessage()
		return "image", im.GetCaption(), &Media{Kind: "image", Mime: im.GetMimetype(), Size: im.GetFileLength()},
			&waE2E.Message{ImageMessage: im}, true
	case m.GetVideoMessage() != nil:
		v := m.GetVideoMessage()
		return "video", v.GetCaption(), &Media{Kind: "video", Mime: v.GetMimetype(), Size: v.GetFileLength()},
			&waE2E.Message{VideoMessage: v}, true
	case m.GetAudioMessage() != nil:
		a := m.GetAudioMessage()
		kind := "audio"
		if a.GetPTT() {
			kind = "voice"
		}
		return kind, "", &Media{Kind: kind, Mime: a.GetMimetype(), Size: a.GetFileLength()},
			&waE2E.Message{AudioMessage: a}, true
	case m.GetDocumentMessage() != nil:
		d := m.GetDocumentMessage()
		return "file", d.GetCaption(), &Media{Kind: "file", Mime: d.GetMimetype(), Name: d.GetFileName(), Size: d.GetFileLength()},
			&waE2E.Message{DocumentMessage: d}, true
	case m.GetStickerMessage() != nil:
		s := m.GetStickerMessage()
		return "sticker", "", &Media{Kind: "sticker", Mime: s.GetMimetype(), Size: s.GetFileLength()},
			&waE2E.Message{StickerMessage: s}, true
	case m.GetLocationMessage() != nil:
		l := m.GetLocationMessage()
		return "location", locationText(l.GetName(), l.GetAddress(), l.GetDegreesLatitude(), l.GetDegreesLongitude()), nil, nil, true
	case m.GetLiveLocationMessage() != nil:
		l := m.GetLiveLocationMessage()
		return "location", locationText("", l.GetCaption(), l.GetDegreesLatitude(), l.GetDegreesLongitude()), nil, nil, true
	case m.GetContactMessage() != nil:
		return "contact", m.GetContactMessage().GetDisplayName(), nil, nil, true
	case m.GetContactsArrayMessage() != nil:
		var names []string
		for _, c := range m.GetContactsArrayMessage().GetContacts() {
			names = append(names, c.GetDisplayName())
		}
		return "contact", strings.Join(names, ", "), nil, nil, true
	}
	if p := pollOf(m); p != nil {
		lines := []string{p.GetName()}
		for _, o := range p.GetOptions() {
			lines = append(lines, "• "+o.GetOptionName())
		}
		return "poll", strings.Join(lines, "\n"), nil, nil, true
	}
	return "", "", nil, nil, false
}

func pollOf(m *waE2E.Message) *waE2E.PollCreationMessage {
	for _, p := range []*waE2E.PollCreationMessage{m.GetPollCreationMessage(), m.GetPollCreationMessageV2(), m.GetPollCreationMessageV3()} {
		if p != nil {
			return p
		}
	}
	return nil
}

func locationText(name, address string, lat, long float64) string {
	var parts []string
	for _, p := range []string{name, address} {
		if p != "" {
			parts = append(parts, p)
		}
	}
	parts = append(parts, fmt.Sprintf("https://www.openstreetmap.org/?mlat=%.6f&mlon=%.6f", lat, long))
	return strings.Join(parts, "\n")
}

// Convert maps one whatsmeow message (live or from history, already unwrapped) to what the daemon needs.
func Convert(evt *events.Message, canon Canon) Outcome {
	info := evt.Info
	m := evt.Message
	if m == nil || chatSkipped(info.Chat) {
		return Outcome{Kind: skip}
	}
	base := Message{
		ID:        info.ID,
		Chat:      canon(info.Chat).String(),
		FromMe:    info.IsFromMe,
		Timestamp: info.Timestamp.UnixMilli(),
	}
	if !info.IsFromMe {
		sender := info.Sender
		// A LID sender with its phone-number alternative: prefer the phone number.
		if sender.Server == types.HiddenUserServer && info.SenderAlt.Server == types.DefaultUserServer {
			sender = info.SenderAlt
		}
		base.Sender = canon(sender).String()
	}
	if r := m.GetReactionMessage(); r != nil {
		return Outcome{Kind: reaction, Message: base, Target: r.GetKey().GetID(), Emoji: r.GetText()}
	}
	if p := m.GetProtocolMessage(); p != nil {
		switch p.GetType() {
		case waE2E.ProtocolMessage_REVOKE:
			return Outcome{Kind: revoke, Message: base, Target: p.GetKey().GetID()}
		case waE2E.ProtocolMessage_MESSAGE_EDIT:
			_, text, _, _, ok := body(p.GetEditedMessage())
			if !ok {
				return Outcome{Kind: skip}
			}
			t := ParseMarkup(text)
			return Outcome{Kind: edit, Message: base, Target: p.GetKey().GetID(), Text: &t}
		}
		return Outcome{Kind: skip}
	}
	if evt.IsEdit {
		// History gives edits already unwrapped, with the edited message's id.
		_, text, _, _, ok := body(m)
		if !ok {
			return Outcome{Kind: skip}
		}
		t := ParseMarkup(text)
		return Outcome{Kind: edit, Message: base, Target: info.ID, Text: &t}
	}
	if noise(m) {
		return Outcome{Kind: skip}
	}
	kind, text, media, part, ok := body(m)
	if !ok {
		// Something WhatsApp has and Conversations does not show yet: say so rather than lose it.
		base.Kind = "unsupported"
		return Outcome{Kind: content, Message: base}
	}
	base.Kind = kind
	if text != "" {
		t := ParseMarkup(text)
		base.Text = &t
	}
	if media != nil {
		media.Ref = info.ID
		base.Media = media
	}
	if ci := contextOf(m); ci != nil && ci.GetStanzaID() != "" {
		base.ReplyTo = ci.GetStanzaID()
	}
	return Outcome{Kind: content, Message: base, MediaProto: part}
}
