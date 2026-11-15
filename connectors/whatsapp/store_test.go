package main

import (
	"context"
	"path/filepath"
	"sort"
	"testing"
	"time"
)

func TestUsersSurvivePruningAndSendsAreIdempotent(t *testing.T) {
	ctx := context.Background()
	l, err := OpenLocal(filepath.Join(t.TempDir(), "connector.db"))
	if err != nil {
		t.Fatal(err)
	}
	defer l.Close()
	old := time.Now().Add(-2 * keepFor).UnixMilli()
	l.Remember(ctx, "a@s.whatsapp.net", "m1", "a@s.whatsapp.net", "a@s.whatsapp.net", false, old, "old", nil)
	l.Remember(ctx, "g@g.us", "m2", "g@g.us", "b@s.whatsapp.net", false, time.Now().UnixMilli(), "x", nil)
	l.Seen(ctx, "c@s.whatsapp.net")
	l.Prune(ctx)
	if _, ok := l.Known(ctx, "a@s.whatsapp.net", "m1"); ok {
		t.Fatal("old messages are pruned")
	}
	users := l.Users(ctx)
	sort.Strings(users)
	if len(users) != 1 || users[0] != "c@s.whatsapp.net" {
		t.Fatalf("users %v (groups excluded, pruned chats only kept when seen)", users)
	}
	if err := l.RememberSent(ctx, "client-1", "ID1"); err != nil {
		t.Fatal(err)
	}
	_ = l.RememberSent(ctx, "client-1", "ID2")
	if id, ok := l.SentID(ctx, "client-1"); !ok || id != "ID1" {
		t.Fatalf("sent id %q %v", id, ok)
	}
}
