package main

import (
	"bytes"
	"encoding/binary"
)

// oggSeconds is the length of an Ogg Opus file in seconds (rounded up): the granule position of the last page, at 48 kHz.
func oggSeconds(data []byte) uint32 {
	i := bytes.LastIndex(data, []byte("OggS"))
	if i < 0 || i+14 > len(data) {
		return 1
	}
	granule := binary.LittleEndian.Uint64(data[i+6 : i+14])
	if granule == 0 || granule == ^uint64(0) {
		return 1
	}
	return uint32((granule + 47999) / 48000)
}
