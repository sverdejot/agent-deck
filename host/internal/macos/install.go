package macos

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
)

const agentLabel = "com.agent-deck.host"

var home, _ = os.UserHomeDir()

// AgentPath is the LaunchAgent plist.
func AgentPath() string { return filepath.Join(home, "Library", "LaunchAgents", agentLabel+".plist") }

// LogPath is where the LaunchAgent writes agent-deck's output.
func LogPath() string { return filepath.Join(home, "Library", "Logs", "agent-deck.log") }

// InstallBinary copies the running binary to ~/.local/bin/agent-deck (a path that survives
// rebuilds and moves of the repo) and returns that path.
func InstallBinary() (string, error) {
	src, err := os.Executable()
	if err != nil {
		return "", err
	}
	src, _ = filepath.EvalSymlinks(src)
	dst := filepath.Join(home, ".local", "bin", "agent-deck")
	if src == dst {
		return dst, nil
	}
	b, err := os.ReadFile(src)
	if err != nil {
		return "", err
	}
	if err := os.MkdirAll(filepath.Dir(dst), 0o755); err != nil {
		return "", err
	}
	tmp := dst + ".tmp"
	if err := os.WriteFile(tmp, b, 0o755); err != nil {
		return "", err
	}
	return dst, os.Rename(tmp, dst) // rename: a running copy keeps its old inode
}

// InstallLaunchAgent runs exe at login, restarts it if it dies, and starts it now.
func InstallLaunchAgent(exe string) error {
	plist := fmt.Sprintf(`<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>Label</key><string>%s</string>
	<key>ProgramArguments</key><array><string>%s</string></array>
	<key>RunAtLoad</key><true/>
	<key>KeepAlive</key><true/>
	<key>ProcessType</key><string>Interactive</string>
	<key>EnvironmentVariables</key><dict>
		<key>PATH</key><string>/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin</string>
	</dict>
	<key>StandardOutPath</key><string>%s</string>
	<key>StandardErrorPath</key><string>%s</string>
</dict>
</plist>
`, agentLabel, exe, LogPath(), LogPath())
	if err := os.WriteFile(AgentPath(), []byte(plist), 0o644); err != nil {
		return err
	}
	domain := fmt.Sprintf("gui/%d", os.Getuid())
	_ = exec.Command("launchctl", "bootout", domain+"/"+agentLabel).Run() // not loaded yet: fine
	return run("launchctl", "bootstrap", domain, AgentPath())
}

// UninstallLaunchAgent stops agent-deck and removes it from login.
func UninstallLaunchAgent() error {
	domain := fmt.Sprintf("gui/%d", os.Getuid())
	_ = exec.Command("launchctl", "bootout", domain+"/"+agentLabel).Run()
	if err := os.Remove(AgentPath()); err != nil && !os.IsNotExist(err) {
		return err
	}
	return nil
}
