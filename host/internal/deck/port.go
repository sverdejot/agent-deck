package deck

import (
	"os"
	"path/filepath"
	"strings"

	"go.bug.st/serial"
	"go.bug.st/serial/enumerator"
)

// The Indicator's ESP32-S3 is behind a CH340 USB-serial chip.
const (
	ch340VID = "1a86"
	ch340PID = "7523"
	Baud     = 115200
)

// FindPort returns the deck's serial port, or "" if it isn't plugged in. A non-empty override
// is used as is (when it exists).
func FindPort(override string) string {
	if override != "" {
		if _, err := os.Stat(override); err == nil {
			return override
		}
		return ""
	}
	if ports, err := enumerator.GetDetailedPortsList(); err == nil {
		for _, p := range ports {
			if p.IsUSB && strings.EqualFold(p.VID, ch340VID) && strings.EqualFold(p.PID, ch340PID) && strings.HasPrefix(p.Name, "/dev/cu.") {
				return p.Name
			}
		}
	}
	if m, _ := filepath.Glob("/dev/cu.usbserial-*"); len(m) > 0 {
		return m[0]
	}
	return ""
}

// Open opens the port without touching DTR/RTS: on the Indicator they drive EN/IO0 (the
// auto-reset circuit), so toggling them would reboot the deck.
func Open(name string) (serial.Port, error) {
	return serial.Open(name, &serial.Mode{
		BaudRate:          Baud,
		InitialStatusBits: &serial.ModemOutputBits{DTR: false, RTS: false},
	})
}
