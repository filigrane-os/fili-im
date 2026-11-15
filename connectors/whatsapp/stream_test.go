package main

import (
	"bytes"
	"crypto/aes"
	"crypto/cipher"
	"crypto/sha256"
	"fmt"
	"io"
	"net/http"
	"net/http/httptest"
	"strconv"
	"strings"
	"testing"

	"golang.org/x/crypto/hkdf"
)

// encrypt makes a file the way WhatsApp stores it: AES-256-CBC (PKCS7) then a 10-byte authentication code.
func encrypt(t *testing.T, key []byte, info string, plain []byte) []byte {
	expanded := make([]byte, 112)
	if _, err := io.ReadFull(hkdf.New(sha256.New, key, nil, []byte(info)), expanded); err != nil {
		t.Fatal(err)
	}
	pad := 16 - len(plain)%16
	padded := append(append([]byte{}, plain...), bytes.Repeat([]byte{byte(pad)}, pad)...)
	block, _ := aes.NewCipher(expanded[16:48])
	out := make([]byte, len(padded))
	cipher.NewCBCEncrypter(block, expanded[:16]).CryptBlocks(out, padded)
	return append(out, make([]byte, 10)...)
}

func TestStreamDecryptsRanges(t *testing.T) {
	key := bytes.Repeat([]byte{7}, 32)
	plain := make([]byte, 3*streamChunk/2+123) // more than one chunk, not a multiple of 16
	for i := range plain {
		plain[i] = byte(i * 31)
	}
	enc := encrypt(t, key, "WhatsApp Video Keys", plain)
	requests := 0
	cdn := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		requests++
		spec := strings.TrimPrefix(r.Header.Get("Range"), "bytes=")
		parts := strings.SplitN(spec, "-", 2)
		a, _ := strconv.Atoi(parts[0])
		b, _ := strconv.Atoi(parts[1])
		b = min(b, len(enc)-1)
		w.WriteHeader(http.StatusPartialContent)
		w.Write(enc[a : b+1])
	}))
	defer cdn.Close()

	var s streamer
	addr, err := s.add(&streamSource{url: cdn.URL, key: key, info: "WhatsApp Video Keys", length: int64(len(plain)), mime: "video/mp4"})
	if err != nil {
		t.Fatal(err)
	}
	get := func(rng string) []byte {
		req, _ := http.NewRequest("GET", addr, nil)
		if rng != "" {
			req.Header.Set("Range", rng)
		}
		resp, err := http.DefaultClient.Do(req)
		if err != nil {
			t.Fatal(err)
		}
		defer resp.Body.Close()
		body, _ := io.ReadAll(resp.Body)
		return body
	}
	if got := get(""); !bytes.Equal(got, plain) {
		t.Fatalf("whole file differs (%d bytes, want %d)", len(got), len(plain))
	}
	for _, c := range [][2]int{{0, 0}, {5, 20}, {16, 31}, {17, 17}, {streamChunk - 3, streamChunk + 40}, {len(plain) - 5, len(plain) - 1}} {
		got := get(fmt.Sprintf("bytes=%d-%d", c[0], c[1]))
		if !bytes.Equal(got, plain[c[0]:c[1]+1]) {
			t.Fatalf("range %v differs", c)
		}
	}
	if got := get(fmt.Sprintf("bytes=%d-", len(plain)-10)); !bytes.Equal(got, plain[len(plain)-10:]) {
		t.Fatal("open-ended range differs")
	}
	if got := get("bytes=-7"); !bytes.Equal(got, plain[len(plain)-7:]) {
		t.Fatal("suffix range differs")
	}
	if requests == 0 {
		t.Fatal("nothing was fetched")
	}
}
