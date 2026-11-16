package main

import (
	"encoding/binary"
	"testing"
)

func TestOggSeconds(t *testing.T) {
	page := make([]byte, 27)
	copy(page, "OggS")
	binary.LittleEndian.PutUint64(page[6:], 48000*3+100) // 3.002 s
	file := append([]byte("OggS\x00\x02headerpadding...."), page...)
	if got := oggSeconds(file); got != 4 {
		t.Fatalf("got %d, want 4", got)
	}
	if got := oggSeconds([]byte("nothing")); got != 1 {
		t.Fatalf("got %d for a file without pages", got)
	}
}
