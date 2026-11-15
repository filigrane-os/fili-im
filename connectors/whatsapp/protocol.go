// Connector protocol v1 (docs/connector-protocol.md): the shapes this connector writes and reads.
package main

import (
	"encoding/json"
	"io"
	"sync"
)

const protocolVersion = 1

type Span struct {
	Start   int    `json:"start"`
	End     int    `json:"end"`
	Style   string `json:"style"`
	URL     string `json:"url,omitempty"`
	Contact string `json:"contact,omitempty"`
}

type Text struct {
	Plain string `json:"plain"`
	Spans []Span `json:"spans,omitempty"`
}

type Media struct {
	Kind string `json:"kind"`
	Mime string `json:"mime,omitempty"`
	Name string `json:"name,omitempty"`
	Size uint64 `json:"size,omitempty"`
	Ref  string `json:"ref"`
}

type Message struct {
	ID        string `json:"id"`
	Chat      string `json:"chat"`
	Sender    string `json:"sender,omitempty"`
	FromMe    bool   `json:"from_me"`
	Timestamp int64  `json:"timestamp"`
	Kind      string `json:"kind"`
	Text      *Text  `json:"text,omitempty"`
	ReplyTo   string `json:"reply_to,omitempty"`
	Media     *Media `json:"media,omitempty"`
	ClientID  string `json:"client_id,omitempty"`
}

type Chat struct {
	ID         string  `json:"id"`
	Kind       string  `json:"kind,omitempty"`
	Title      string  `json:"title,omitempty"`
	Unread     *int64  `json:"unread,omitempty"`
	MutedUntil *int64  `json:"muted_until,omitempty"`
	Archived   *bool   `json:"archived,omitempty"`
	Pinned     *bool   `json:"pinned,omitempty"`
	LastRead   *string `json:"last_read,omitempty"`
}

type Contact struct {
	ID    string `json:"id"`
	Name  string `json:"name,omitempty"`
	Phone string `json:"phone,omitempty"`
}

type ErrorBody struct {
	Code    string `json:"code"`
	Message string `json:"message"`
}

// Command is what the daemon sends; parameters are read from the same line by each handler.
type Command struct {
	ID       uint64          `json:"id"`
	Cmd      string          `json:"cmd"`
	Method   string          `json:"method"`
	Phone    string          `json:"phone"`
	Chat     string          `json:"chat"`
	ClientID string          `json:"client_id"`
	Text     *Text           `json:"text"`
	ReplyTo  string          `json:"reply_to"`
	Message  string          `json:"message"`
	Messages []string        `json:"messages"`
	Sender   string          `json:"sender"`
	Emoji    string          `json:"emoji"`
	Everyone bool            `json:"for_everyone"`
	Active   bool            `json:"active"`
	Ref      string          `json:"ref"`
	Dest     string          `json:"dest"`
	Until    int64           `json:"until"`
	Archived bool            `json:"archived"`
	Media    json.RawMessage `json:"media"`
	State    string          `json:"state"`
}

// Out writes protocol lines; safe from any goroutine (whatsmeow's handlers, command goroutines).
type Out struct {
	mu  sync.Mutex
	enc *json.Encoder
}

func NewOut(w io.Writer) *Out {
	enc := json.NewEncoder(w)
	enc.SetEscapeHTML(false)
	return &Out{enc: enc}
}

func (o *Out) write(v any) {
	o.mu.Lock()
	defer o.mu.Unlock()
	_ = o.enc.Encode(v)
}

// Event writes {"event": name, ...fields}.
func (o *Out) Event(name string, fields map[string]any) {
	line := map[string]any{"event": name}
	for k, v := range fields {
		line[k] = v
	}
	o.write(line)
}

func (o *Out) State(state, detail string, self map[string]any) {
	fields := map[string]any{"state": state}
	if detail != "" {
		fields["detail"] = detail
	}
	if self != nil {
		fields["self"] = self
	}
	o.Event("state", fields)
}

func (o *Out) Reply(id uint64, result any, err *ErrorBody) {
	if err != nil {
		o.write(map[string]any{"reply": id, "ok": false, "error": err})
		return
	}
	if result == nil {
		result = map[string]any{}
	}
	o.write(map[string]any{"reply": id, "ok": true, "result": result})
}

func capabilities() map[string]any {
	return map[string]any{
		"pairing":         []string{"qr", "code"},
		"message_kinds":   []string{"text", "image", "video", "file"},
		"media_max_bytes": 104857600,
		"edit":            map[string]any{"window_s": 900},
		"delete":          map[string]any{"window_s": 172800},
		"reactions":       "one_per_user",
		"replies":         true,
		"threads":         false,
		"receipts":        map[string]any{"send": true, "suppress": true},
		"typing":          map[string]any{"send": true, "receive": true},
		"history":         "on_pairing",
		"chat_kinds":      []string{"direct", "group"},
		"formatting":      "spans",
		"identity":        map[string]any{"phone": true},
		"presence":        true,
		"avatars":         true,
		"streaming":       true,
	}
}
