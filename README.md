# Stream Grep

A simple terminal application to run a command and filter its output in real-time.

## Features

- Run any command and view its output in a terminal UI
- Real-time filtering/search of command output
- Split-view terminal interface
- Keyboard navigation between panels

## Usage

```
cargo run <command> [args...]
```

For example:

```
cargo run node tick.js
```

## Keyboard Shortcuts

- `Tab` - Cycle through panels
- `Shift+Tab` - Cycle through panels (reverse)
- `Esc` or `Ctrl+C` - Exit the application

When in search box:
- Arrow keys to move cursor
- Type to enter search pattern
- Enter to apply search

## Building

```
cargo build --release
```

## License

MIT