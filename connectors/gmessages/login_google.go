// Pairing with a Google account, the way Google Messages for web now works (the QR code was removed from the phone
// app): the user signs in to Google in a browser window opened on a throw-away profile; the browser's session cookies
// are read over its debugging port, the profile is deleted, and the phone asks to confirm an emoji.
package main

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strconv"
	"strings"
	"time"

	"github.com/coder/websocket"
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

// googleCookies opens the sign-in window and returns the Google cookies once the user is signed in.
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
	cmd := exec.CommandContext(ctx, bin, "--user-data-dir="+profile, "--remote-debugging-port=0", "--no-first-run",
		"--no-default-browser-check", "--password-store=basic", "--new-window", googleLoginURL)
	cmd.Stdout, cmd.Stderr = nil, nil
	if err := cmd.Start(); err != nil {
		return nil, fmt.Errorf("cannot start the browser: %w", err)
	}
	exited := make(chan struct{})
	go func() { _ = cmd.Wait(); close(exited) }()
	defer func() {
		<-exited
	}()
	wsURL, err := debugURL(ctx, profile, exited)
	if err != nil {
		_ = cmd.Process.Kill()
		return nil, err
	}
	conn, _, err := websocket.Dial(ctx, wsURL, nil)
	if err != nil {
		_ = cmd.Process.Kill()
		return nil, fmt.Errorf("cannot talk to the browser: %w", err)
	}
	conn.SetReadLimit(8 << 20)
	defer conn.CloseNow()
	closeBrowser := func() {
		_ = conn.Write(context.Background(), websocket.MessageText, []byte(`{"id":99,"method":"Browser.close"}`))
		select {
		case <-exited:
		case <-time.After(3 * time.Second):
			_ = cmd.Process.Kill()
		}
	}
	for id := 1; ; id++ {
		select {
		case <-ctx.Done():
			closeBrowser()
			return nil, ctx.Err()
		case <-exited:
			return nil, errors.New("the sign-in window was closed")
		case <-time.After(2 * time.Second):
		}
		cookies, err := fetchCookies(ctx, conn, id)
		if err != nil {
			select {
			case <-exited:
				return nil, errors.New("the sign-in window was closed")
			default:
			}
			continue
		}
		if complete(cookies) {
			closeBrowser()
			return cookies, nil
		}
	}
}

func debugURL(ctx context.Context, profile string, exited <-chan struct{}) (string, error) {
	deadline := time.After(20 * time.Second)
	for {
		if raw, err := os.ReadFile(filepath.Join(profile, "DevToolsActivePort")); err == nil {
			lines := strings.Split(strings.TrimSpace(string(raw)), "\n")
			if len(lines) >= 2 {
				if _, err := strconv.Atoi(strings.TrimSpace(lines[0])); err == nil {
					return "ws://127.0.0.1:" + strings.TrimSpace(lines[0]) + strings.TrimSpace(lines[1]), nil
				}
			}
		}
		select {
		case <-ctx.Done():
			return "", ctx.Err()
		case <-exited:
			return "", errors.New("the browser exited at once (is another window of it blocking a new profile?)")
		case <-deadline:
			return "", errors.New("the browser did not start in time")
		case <-time.After(200 * time.Millisecond):
		}
	}
}

func fetchCookies(ctx context.Context, conn *websocket.Conn, id int) (map[string]string, error) {
	ctx, cancel := context.WithTimeout(ctx, 10*time.Second)
	defer cancel()
	req, _ := json.Marshal(map[string]any{"id": id, "method": "Storage.getCookies"})
	if err := conn.Write(ctx, websocket.MessageText, req); err != nil {
		return nil, err
	}
	for {
		_, raw, err := conn.Read(ctx)
		if err != nil {
			return nil, err
		}
		var resp struct {
			ID     int `json:"id"`
			Result struct {
				Cookies []struct {
					Name   string `json:"name"`
					Value  string `json:"value"`
					Domain string `json:"domain"`
				} `json:"cookies"`
			} `json:"result"`
		}
		if json.Unmarshal(raw, &resp) != nil || resp.ID != id {
			continue
		}
		out := map[string]string{}
		for _, ck := range resp.Result.Cookies {
			wanted := false
			for _, n := range append(append([]string{}, requiredCookies...), optionalCookies...) {
				wanted = wanted || n == ck.Name
			}
			if !wanted {
				continue
			}
			// OSID lives on messages.google.com, the others on .google.com (an exact domain beats a lookalike).
			good := (ck.Name == "OSID" && strings.HasSuffix(ck.Domain, "messages.google.com")) ||
				(ck.Name != "OSID" && ck.Domain == ".google.com")
			if _, have := out[ck.Name]; good || !have {
				out[ck.Name] = ck.Value
			}
		}
		return out, nil
	}
}

func complete(cookies map[string]string) bool {
	for _, n := range requiredCookies {
		if cookies[n] == "" {
			return false
		}
	}
	return true
}
