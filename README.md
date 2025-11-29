# Current Location

A tool that help to determine Current Working File of currently active window which goes beyond
just looking up CWD of a process

Supported window managers:

- [Hyprland](https://hypr.land/)

## How It Works

Every program that user wants to integrate this tool with must use `current-location write` (see
`current-location write --help`) command to write it's current location to a 'location registry'
every time Current Working File is changed. Examples of such programs:

- Editor (NeoVim, VS Code)
- User shell (Zsh)

Those recorded locations can then be used by calling `current-location get` to launch other
programs with the same Current Working File, examples:

- Terminal Emulator: open new window in the same directory
- File Manager (NNN): open file manager pointing on the currently edited file
- Git Manager (LazyGit): open Git window of current repository

## Nix

Nix and NixOS users may try this tool without installing it:

``` sh
nix run github:aitvann/current-location -- --help
```
