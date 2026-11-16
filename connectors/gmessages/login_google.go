// Pairing with a Google account, the way Google Messages for web now works (the QR code was removed from the phone
// app): the user signs in to Google in a browser window opened on a throw-away profile; the profile's session cookies
// are read, the profile is deleted, and the phone asks to confirm an emoji.
package main

import (
	"context"
	"crypto/aes"
	"crypto/cipher"
	"crypto/hmac"
	"crypto/sha1"
	"crypto/sha256"
	"database/sql"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"syscall"
	"time"

	_ "github.com/mattn/go-sqlite3"
)

const googleLoginURL = "https://accounts.google.com/AccountChooser?continue=https://messages.google.com/web/config"

// Cookies the Messages for web session needs (the same set as mautrix-gmessages).
var requiredCookies = []string{"SID", "HSID", "SSID", "APISID", "SAPISID", "OSID"}
var optionalCookies = []string{"__Secure-1PSIDTS"}

var browsers = []string{"google-chrome-stable", "google-chrome", "chromium", "chromium-browser", "brave-browser", "microsoft-edge-stable", "vivaldi"}

var errNoBrowser = errors.New("a Chromium-based browser (Chrome, Chromium, Brave, Edge) is needed to sign in to Google")

func findBrowser() (string, error) {
	// FILI_IM_BROWSER names the program to use (tests set it to a stub).
	if b := os.Getenv("FILI_IM_BROWSER"); b != "" {
		return exec.LookPath(b)
	}
	for _, b := range browsers {
		if p, err := exec.LookPath(b); err == nil {
			return p, nil
		}
	}
	return "", errNoBrowser
}

// googleCookies opens the sign-in window and returns the Google cookies once the user is signed in. The browser runs
// without any automation switch (Google refuses to sign in a browser that is remote-controlled); its throw-away
// profile uses the basic password store, so its cookie file is readable with the fixed key Chromium then uses.
func googleCookies(ctx context.Context, dataDir string) (map[string]string, error) {
	bin, err := findBrowser()
	if err != nil {
		return nil, err
	}
	profile, err := os.MkdirTemp(dataDir, "login-")
	if err != nil {
		return nil, err
	}
	defer os.RemoveAll(profile)
	cmd := exec.Command(bin, "--user-data-dir="+profile, "--no-first-run", "--no-default-browser-check",
		"--password-store=basic", "--new-window", googleLoginURL)
	cmd.Stdout, cmd.Stderr = nil, nil
	if err := cmd.Start(); err != nil {
		return nil, fmt.Errorf("cannot start the browser: %w", err)
	}
	exited := make(chan struct{})
	go func() { _ = cmd.Wait(); close(exited) }()
	stopBrowser := func() {
		_ = cmd.Process.Signal(syscall.SIGTERM)
		select {
		case <-exited:
		case <-time.After(5 * time.Second):
			_ = cmd.Process.Kill()
			<-exited
		}
	}
	for {
		select {
		case <-ctx.Done():
			stopBrowser()
			return nil, ctx.Err()
		case <-exited:
			// The last look: the browser writes its cookies when it quits.
			if cookies, err := readCookies(profile); err == nil && complete(cookies) {
				return cookies, nil
			}
			return nil, errors.New("the sign-in window was closed")
		case <-time.After(3 * time.Second):
		}
		if cookies, err := readCookies(profile); err == nil && complete(cookies) {
			stopBrowser()
			return cookies, nil
		}
	}
}

// readCookies reads (a copy of) the profile's cookie database and decrypts the Google cookies.
func readCookies(profile string) (map[string]string, error) {
	var src string
	for _, p := range []string{"Default/Network/Cookies", "Default/Cookies"} {
		if _, err := os.Stat(filepath.Join(profile, p)); err == nil {
			src = filepath.Join(profile, p)
			break
		}
	}
	if src == "" {
		return nil, errors.New("no cookie file yet")
	}
	tmp, err := os.MkdirTemp(filepath.Dir(profile), "cookies-")
	if err != nil {
		return nil, err
	}
	defer os.RemoveAll(tmp)
	for _, suffix := range []string{"", "-wal", "-journal"} {
		if raw, err := os.ReadFile(src + suffix); err == nil {
			_ = os.WriteFile(filepath.Join(tmp, "Cookies"+suffix), raw, 0o600)
		}
	}
	db, err := sql.Open("sqlite3", "file:"+filepath.Join(tmp, "Cookies")+"?mode=ro")
	if err != nil {
		return nil, err
	}
	defer db.Close()
	rows, err := db.Query(`SELECT host_key, name, value, encrypted_value FROM cookies WHERE host_key LIKE '%google.com'`)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	out := map[string]string{}
	for rows.Next() {
		var host, name, value string
		var enc []byte
		if rows.Scan(&host, &name, &value, &enc) != nil {
			continue
		}
		wanted := false
		for _, n := range append(append([]string{}, requiredCookies...), optionalCookies...) {
			wanted = wanted || n == name
		}
		if !wanted {
			continue
		}
		if value == "" && len(enc) > 0 {
			plain, err := decryptCookie(host, enc)
			if err != nil {
				continue
			}
			value = plain
		}
		// OSID lives on messages.google.com, the others on .google.com (an exact domain beats a lookalike).
		good := (name == "OSID" && strings.HasSuffix(host, "messages.google.com")) || (name != "OSID" && host == ".google.com")
		if _, have := out[name]; good || !have {
			out[name] = value
		}
	}
	return out, rows.Err()
}

// cookieKey is PBKDF2-SHA1("peanuts", "saltysalt", 1 round, 16 bytes): one HMAC block.
func cookieKey() []byte {
	mac := hmac.New(sha1.New, []byte("peanuts"))
	mac.Write([]byte("saltysalt\x00\x00\x00\x01"))
	return mac.Sum(nil)[:16]
}

// decryptCookie opens a "v10" Chromium cookie: AES-128-CBC, key PBKDF2-SHA1("peanuts", "saltysalt", 1 round), IV of
// 16 spaces. Recent versions put SHA-256 of the host in front of the value.
func decryptCookie(host string, enc []byte) (string, error) {
	if len(enc) < 3+aes.BlockSize || (string(enc[:3]) != "v10" && string(enc[:3]) != "v11") {
		return "", errors.New("unknown cookie encryption")
	}
	if string(enc[:3]) == "v11" {
		return "", errors.New("keyring-encrypted cookie")
	}
	block, err := aes.NewCipher(cookieKey())
	if err != nil {
		return "", err
	}
	data := enc[3:]
	if len(data)%aes.BlockSize != 0 {
		return "", errors.New("bad cookie length")
	}
	plain := make([]byte, len(data))
	cipher.NewCBCDecrypter(block, []byte("                ")).CryptBlocks(plain, data)
	pad := int(plain[len(plain)-1])
	if pad < 1 || pad > aes.BlockSize || pad > len(plain) {
		return "", errors.New("bad cookie padding")
	}
	plain = plain[:len(plain)-pad]
	if sum := sha256.Sum256([]byte(host)); len(plain) >= 32 && string(plain[:32]) == string(sum[:]) {
		plain = plain[32:]
	}
	return string(plain), nil
}

func complete(cookies map[string]string) bool {
	for _, n := range requiredCookies {
		if cookies[n] == "" {
			return false
		}
	}
	return true
}
