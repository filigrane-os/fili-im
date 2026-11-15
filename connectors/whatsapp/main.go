// fili-im-whatsapp: the WhatsApp connector of Conversations (fili-im), on whatsmeow, as a linked device.
//
// Started by the daemon: fili-im-whatsapp --account <key> --data <dir>. Protocol on stdin/stdout
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
	"syscall"

	_ "github.com/mattn/go-sqlite3"
	waLog "go.mau.fi/whatsmeow/util/log"
)

var version = "0.1.0"

// stderrLog: whatsmeow's log goes to stderr (stdout is the protocol), at WARN unless FILI_IM_DEBUG is set. Debug
// output may contain message contents: never on by default.
type stderrLog struct {
	module string
	debug  bool
}

func (l stderrLog) printf(level, msg string, args ...any) {
	fmt.Fprintf(os.Stderr, "fili-im-whatsapp [%s %s] %s\n", l.module, level, fmt.Sprintf(msg, args...))
}
func (l stderrLog) Warnf(msg string, args ...any)  { l.printf("WARN", msg, args...) }
func (l stderrLog) Errorf(msg string, args ...any) { l.printf("ERROR", msg, args...) }
func (l stderrLog) Infof(msg string, args ...any) {
	if l.debug {
		l.printf("INFO", msg, args...)
	}
}
func (l stderrLog) Debugf(msg string, args ...any) {}
func (l stderrLog) Sub(module string) waLog.Logger {
	return stderrLog{module: l.module + "/" + module, debug: l.debug}
}

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
		fmt.Fprintln(os.Stderr, "fili-im-whatsapp: --data is required (this program is started by fili-im daemon)")
		os.Exit(2)
	}
	_ = account
	out := NewOut(os.Stdout)
	out.Event("hello", map[string]any{
		"protocol": protocolVersion, "platform": "whatsapp", "name": "WhatsApp", "version": version, "capabilities": capabilities(),
	})
	log := stderrLog{module: "WA", debug: os.Getenv("FILI_IM_DEBUG") != ""}
	conn, err := NewConnector(out, *data, log)
	if err != nil {
		out.State("error", err.Error(), nil)
		// Still answer commands (with errors) until the daemon stops us, so it does not restart in a loop.
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
