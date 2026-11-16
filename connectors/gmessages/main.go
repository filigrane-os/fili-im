// fili-im-gmessages: the Google Messages connector of Conversations (fili-im), on libgm (the "Messages for web"
// protocol of mautrix-gmessages): the phone's SMS, MMS and RCS conversations, linked like a browser by a QR code.
//
// Started by the daemon: fili-im-gmessages --account <key> --data <dir>. Protocol on stdin/stdout
// (docs/connector-protocol.md), log on stderr (never message contents).
package main

import (
	"bufio"
	"encoding/json"
	"errors"
	"flag"
	"fmt"
	"os"
	"os/signal"
	"runtime/debug"
	"syscall"
)

var version = "0.1.0"

func main() {
	account := flag.String("account", "", "account key given by the daemon")
	data := flag.String("data", "", "private directory of the account")
	showVersion := flag.Bool("version", false, "print the version")
	flag.Parse()
	if *showVersion {
		fmt.Println(version)
		return
	}
	if *data == "" {
		fmt.Fprintln(os.Stderr, "fili-im-gmessages: --data is required (this program is started by fili-im daemon)")
		os.Exit(2)
	}
	_ = account
	debug.SetGCPercent(50)
	debug.SetMemoryLimit(32 << 20)
	out := NewOut(os.Stdout)
	out.Event("hello", map[string]any{
		"protocol": protocolVersion, "platform": "gmessages", "name": "Google Messages", "version": version, "capabilities": capabilities(),
	})
	conn, err := NewConnector(out, *data)
	if err != nil {
		out.State("error", err.Error(), nil)
		serve(out, nil)
		return
	}
	conn.Start()
	signals := make(chan os.Signal, 1)
	signal.Notify(signals, syscall.SIGTERM, syscall.SIGINT)
	go func() {
		<-signals
		conn.Stop()
		os.Exit(0)
	}()
	serve(out, conn)
	conn.Stop()
}

// serve reads commands until shutdown or end of input; each runs in its own goroutine (replies may come in any order).
func serve(out *Out, conn *Connector) {
	scanner := bufio.NewScanner(os.Stdin)
	scanner.Buffer(make([]byte, 64*1024), 16*1024*1024)
	for scanner.Scan() {
		var cmd Command
		if err := json.Unmarshal(scanner.Bytes(), &cmd); err != nil {
			out.Reply(0, nil, &ErrorBody{Code: "invalid", Message: "not a command: " + err.Error()})
			continue
		}
		if cmd.Cmd == "shutdown" {
			out.Reply(cmd.ID, nil, nil)
			return
		}
		if conn == nil {
			out.Reply(cmd.ID, nil, &ErrorBody{Code: "failed", Message: "the connector could not start; see its log"})
			continue
		}
		go func(cmd Command) {
			defer func() {
				if r := recover(); r != nil {
					out.Reply(cmd.ID, nil, &ErrorBody{Code: "failed", Message: fmt.Sprintf("internal error in %s", cmd.Cmd)})
				}
			}()
			result, err := conn.Run(&cmd)
			if err != nil {
				var ce *cmdError
				if errors.As(err, &ce) {
					out.Reply(cmd.ID, nil, &ErrorBody{Code: ce.code, Message: ce.message})
				} else {
					out.Reply(cmd.ID, nil, &ErrorBody{Code: "failed", Message: err.Error()})
				}
				return
			}
			out.Reply(cmd.ID, result, nil)
		}(cmd)
	}
}
