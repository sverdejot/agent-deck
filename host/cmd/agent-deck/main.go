// agent-deck feeds the Claude Code terminal sessions open on this Mac to the Agent Deck
// (SenseCAP Indicator) over USB serial, and brings to the front the one tapped on the screen.
//
//	agent-deck                       run: waits for the deck, reconnects on its own
//	agent-deck list [-v]             print the sessions it sees (-v: with summaries)
//	agent-deck install               run at login (LaunchAgent) and start now
//	agent-deck uninstall             remove the LaunchAgent
//	agent-deck install-statusline    tap Claude Code's statusline (plan limits, context window)
//	agent-deck uninstall-statusline  restore the original statusline
//	agent-deck statusline -- CMD     (run by Claude Code) record the snapshot, then run CMD
package main

import (
	"flag"
	"fmt"
	"log"
	"os"

	"github.com/sverdejot/agent-deck/host/internal/claude"
	"github.com/sverdejot/agent-deck/host/internal/macos"
)

const usage = "usage: agent-deck [-port /dev/cu.x] [-v] [list|install|uninstall|install-statusline|uninstall-statusline]"

func main() {
	port := flag.String("port", "", "serial port (default: auto-detect the Indicator's CH340)")
	verbose := flag.Bool("v", false, "more output")
	flag.Parse()

	switch flag.Arg(0) {
	case "":
		log.SetFlags(log.Ltime)
		run(*port, *verbose)
	case "list":
		list(*verbose)
	case "install":
		exe, err := macos.InstallBinary()
		must(err)
		must(macos.InstallLaunchAgent(exe))
		fmt.Printf("instalado: %s\nlog: %s\n", macos.AgentPath(), macos.LogPath())
	case "uninstall":
		must(macos.UninstallLaunchAgent())
		fmt.Println("desinstalado")
	case "install-statusline":
		exe, err := macos.InstallBinary()
		must(err)
		must(claude.InstallStatusline(exe))
	case "uninstall-statusline":
		must(claude.UninstallStatusline())
	case "statusline":
		args := flag.Args()[1:]
		if len(args) > 0 && args[0] == "--" {
			args = args[1:]
		}
		claude.Statusline(args)
	default:
		fmt.Fprintln(os.Stderr, usage)
		os.Exit(2)
	}
}

func list(verbose bool) {
	for _, s := range claude.Sessions() {
		fmt.Printf("%-6s %-40.40s %-16s %s\n", s.State, s.Title, s.Place, s.Age)
		if verbose {
			sum := claude.Summarize(s)
			fmt.Printf("       meta:   %s\n       tú:     %.120s\n       claude: %.120s\n       ahora:  %s\n",
				sum.Meta, sum.You, sum.Claude, sum.Tool)
		}
	}
}

func must(err error) {
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}
