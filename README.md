# probe-rs-rtt

Host side implementation of the RTT (Real-Time Transfer) I/O protocol over probe-rs

## TODO

- Virtual terminal support for channel 0
- Support for filters to limit where to scan for the "control block"
  - Specific memory address (range)
  - Nth block only (if it's duplicated somehow)
  - Symbol address from ELF file?
- Support for using multiple channels at once in the CLI
  - Redirect to file?
  - Redirect to socket?
  - An interactive multi-terminal would be nice but would involve implementing half of "tmux"
