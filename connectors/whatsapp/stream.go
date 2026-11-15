package main

// Streaming of received media: WhatsApp files are encrypted (AES-256-CBC, key derived from the message's media key)
// on a CDN that answers range requests. This is a tiny local HTTP server (127.0.0.1, one unguessable path per file)
// that fetches only the encrypted blocks a player asks for and decrypts them, so a video starts playing and seeks
// without being downloaded first. The authentication code at the end of the file is not checked (the whole file would
// be needed); `fetch_media` still verifies what it saves.

import (
	"context"
	"crypto/aes"
	"crypto/cipher"
	"crypto/rand"
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"io"
	"net"
	"net/http"
	"strconv"
	"strings"
	"sync"

	"go.mau.fi/whatsmeow"
	"golang.org/x/crypto/hkdf"
)

const streamChunk = 1 << 20

type streamSource struct {
	url    string
	key    []byte // media key
	info   string // HKDF info ("WhatsApp Video Keys")
	length int64  // plain length
	mime   string
}

type streamer struct {
	mu      sync.Mutex
	ln      net.Listener
	sources map[string]*streamSource
}

func (s *streamer) add(src *streamSource) (string, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.ln == nil {
		ln, err := net.Listen("tcp", "127.0.0.1:0")
		if err != nil {
			return "", err
		}
		s.ln = ln
		s.sources = map[string]*streamSource{}
		go func() { _ = http.Serve(ln, s) }()
	}
	raw := make([]byte, 16)
	if _, err := rand.Read(raw); err != nil {
		return "", err
	}
	token := hex.EncodeToString(raw)
	s.sources[token] = src
	return fmt.Sprintf("http://%s/%s", s.ln.Addr().String(), token), nil
}

func (s *streamer) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	s.mu.Lock()
	src := s.sources[strings.TrimPrefix(r.URL.Path, "/")]
	s.mu.Unlock()
	if src == nil {
		http.NotFound(w, r)
		return
	}
	start, end := int64(0), src.length-1
	status := http.StatusOK
	if h := r.Header.Get("Range"); strings.HasPrefix(h, "bytes=") {
		spec := strings.SplitN(strings.TrimPrefix(h, "bytes="), "-", 2)
		a, errA := strconv.ParseInt(spec[0], 10, 64)
		if errA != nil && spec[0] != "" {
			http.Error(w, "bad range", http.StatusRequestedRangeNotSatisfiable)
			return
		}
		if spec[0] == "" { // suffix: the last n bytes
			n, _ := strconv.ParseInt(spec[1], 10, 64)
			a = max(0, src.length-n)
		}
		start = a
		if len(spec) == 2 && spec[1] != "" && spec[0] != "" {
			if b, err := strconv.ParseInt(spec[1], 10, 64); err == nil {
				end = min(b, src.length-1)
			}
		}
		if start >= src.length || start > end {
			w.Header().Set("Content-Range", fmt.Sprintf("bytes */%d", src.length))
			http.Error(w, "range not satisfiable", http.StatusRequestedRangeNotSatisfiable)
			return
		}
		status = http.StatusPartialContent
	}
	w.Header().Set("Accept-Ranges", "bytes")
	w.Header().Set("Content-Type", src.mime)
	w.Header().Set("Content-Length", strconv.FormatInt(end-start+1, 10))
	if status == http.StatusPartialContent {
		w.Header().Set("Content-Range", fmt.Sprintf("bytes %d-%d/%d", start, end, src.length))
	}
	w.WriteHeader(status)
	if r.Method == http.MethodHead {
		return
	}
	for pos := start; pos <= end; {
		to := min(end, pos+streamChunk-1)
		data, err := src.plain(r.Context(), pos, to)
		if err != nil {
			return // the connection ends short; the player asks again
		}
		if _, err := w.Write(data); err != nil {
			return
		}
		pos = to + 1
	}
}

// plain returns the decrypted bytes [from, to] (inclusive).
func (src *streamSource) plain(ctx context.Context, from, to int64) ([]byte, error) {
	expanded := make([]byte, 112)
	if _, err := io.ReadFull(hkdf.New(sha256.New, src.key, nil, []byte(src.info)), expanded); err != nil {
		return nil, err
	}
	iv, cipherKey := expanded[:16], expanded[16:48]
	first, last := from/16, to/16
	// The block before the first one wanted is its IV; the first block of the file uses the derived IV.
	reqStart := first * 16
	prev := iv
	if first > 0 {
		reqStart -= 16
	}
	reqEnd := (last+1)*16 - 1
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, src.url, nil)
	if err != nil {
		return nil, err
	}
	req.Header.Set("Range", fmt.Sprintf("bytes=%d-%d", reqStart, reqEnd))
	req.Header.Set("Origin", "https://web.whatsapp.com")
	req.Header.Set("Referer", "https://web.whatsapp.com/")
	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusPartialContent && resp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("the media server answered %d", resp.StatusCode)
	}
	body, err := io.ReadAll(io.LimitReader(resp.Body, reqEnd-reqStart+1))
	if err != nil {
		return nil, err
	}
	if first > 0 {
		if len(body) < 16 {
			return nil, io.ErrUnexpectedEOF
		}
		prev, body = body[:16], body[16:]
	}
	body = body[:len(body)/16*16]
	block, err := aes.NewCipher(cipherKey)
	if err != nil {
		return nil, err
	}
	out := make([]byte, len(body))
	cipher.NewCBCDecrypter(block, prev).CryptBlocks(out, body)
	lo := from - first*16
	hi := min(int64(len(out)), to-first*16+1)
	if lo >= hi {
		return nil, io.ErrUnexpectedEOF
	}
	return out[lo:hi], nil
}

// stream registers the media of a message with the local server and returns its address.
func (c *Connector) stream(ctx context.Context, chat string, cmd *Command) (any, error) {
	ref := cmd.Ref
	if ref == "" {
		ref = cmd.Message
	}
	m, err := c.local.Media(ctx, chat, ref)
	if err != nil {
		return nil, fail("not_found", "no media for this message")
	}
	var part whatsmeow.DownloadableMessage
	var info, mime string
	var length int64
	switch {
	case m.GetVideoMessage() != nil:
		v := m.GetVideoMessage()
		part, info, mime, length = v, "WhatsApp Video Keys", v.GetMimetype(), int64(v.GetFileLength())
	case m.GetAudioMessage() != nil:
		a := m.GetAudioMessage()
		part, info, mime, length = a, "WhatsApp Audio Keys", a.GetMimetype(), int64(a.GetFileLength())
	default:
		return nil, fail("unsupported", "only videos and audio stream")
	}
	url := ""
	if u, ok := part.(interface{ GetURL() string }); ok {
		url = u.GetURL()
	}
	if url == "" || length <= 0 || len(part.GetMediaKey()) == 0 {
		return nil, fail("unsupported", "this file cannot be streamed")
	}
	if i := strings.Index(mime, ";"); i >= 0 && strings.HasPrefix(mime, "video/") {
		mime = mime[:i]
	}
	addr, err := c.streams.add(&streamSource{url: url, key: part.GetMediaKey(), info: info, length: length, mime: mime})
	if err != nil {
		return nil, fail("failed", "cannot start the local server: %v", err)
	}
	return map[string]any{"url": addr, "mime": mime}, nil
}
