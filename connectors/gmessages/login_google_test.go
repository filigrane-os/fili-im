package main

import (
	"crypto/aes"
	"crypto/cipher"
	"crypto/sha256"
	"database/sql"
	"encoding/hex"
	_ "github.com/mattn/go-sqlite3"
	"os"
	"testing"
)

func TestCookieKeyIsChromiumsLinuxKey(t *testing.T) {
	if got := hex.EncodeToString(cookieKey()); got != "fd621fe5a2b402539dfa147ca9272778" {
		t.Fatalf("key %s", got)
	}
}

func encrypt(t *testing.T, plain []byte) []byte {
	block, _ := aes.NewCipher(cookieKey())
	pad := aes.BlockSize - len(plain)%aes.BlockSize
	for i := 0; i < pad; i++ {
		plain = append(plain, byte(pad))
	}
	out := make([]byte, len(plain))
	cipher.NewCBCEncrypter(block, []byte("                ")).CryptBlocks(out, plain)
	return append([]byte("v10"), out...)
}

func TestDecryptCookie(t *testing.T) {
	got, err := decryptCookie(".google.com", encrypt(t, []byte("secret-value")))
	if err != nil || got != "secret-value" {
		t.Fatalf("%q %v", got, err)
	}
	sum := sha256.Sum256([]byte(".google.com"))
	got, err = decryptCookie(".google.com", encrypt(t, append(sum[:], "with-host-hash"...)))
	if err != nil || got != "with-host-hash" {
		t.Fatalf("%q %v", got, err)
	}
	if _, err := decryptCookie(".google.com", []byte("v11xxxxxxxxxxxxxxxxxxxx")); err == nil {
		t.Fatal("keyring cookies are not readable")
	}
}

func TestReadCookiesFromProfile(t *testing.T) {
	root := t.TempDir()
	profile := root + "/profile"
	if err := os.MkdirAll(profile+"/Default/Network", 0o755); err != nil {
		t.Fatal(err)
	}
	db, err := sql.Open("sqlite3", "file:"+profile+"/Default/Network/Cookies")
	if err != nil {
		t.Fatal(err)
	}
	if _, err := db.Exec(`CREATE TABLE cookies (host_key TEXT, name TEXT, value TEXT, encrypted_value BLOB)`); err != nil {
		t.Fatal(err)
	}
	add := func(host, name string, enc []byte, plain string) {
		if _, err := db.Exec(`INSERT INTO cookies VALUES (?, ?, ?, ?)`, host, name, plain, enc); err != nil {
			t.Fatal(err)
		}
	}
	for _, n := range []string{"SID", "HSID", "SSID", "APISID", "SAPISID"} {
		add(".google.com", n, encrypt(t, []byte("v-"+n)), "")
	}
	add("messages.google.com", "OSID", nil, "plain-osid")
	add(".google.com", "NID", encrypt(t, []byte("ignored")), "")
	db.Close()
	got, err := readCookies(profile)
	if err != nil || !complete(got) || got["SID"] != "v-SID" || got["OSID"] != "plain-osid" || got["NID"] != "" {
		t.Fatalf("%v %v", got, err)
	}
}
