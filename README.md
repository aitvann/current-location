# Current Location

A tool that helps to determine the Current Working File of the currently active window.

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

## Integrations

### NeoVim

``` lua
vim.api.nvim_create_autocmd('BufEnter', {
    group = vim.api.nvim_create_augroup('current_location', { clear = true }),
    desc = "Integration with current-location script: write current location on every location change",
    callback = function(args)
        local filepath = args.file
        if filepath ~= "" and vim.fn.filereadable(filepath) == 1 then
            vim.system(
                vim.iter({ "current-location", "write", "nvim", filepath, utils.get_ui_pids(), vim.uv.os_getpid(),
                    "--nvim-pipe", vim.v.servername }):flatten():totable(),
                { text = true },
                function(result)
                    if result.code ~= 0 then
                        vim.notify("Error writing current location (" .. result.code .. "): " .. result.stderr,
                            vim.log.levels.ERROR)
                    end
                end
            )
        end
    end,
})
```

### Zsh

``` zsh
# define an array to collect functions run only once
typeset -ag self_destruct_functions=()
function _self_destruct_hook {
  local f
  for f in ${self_destruct_functions}; do
    "$f"
  done

  # remove self from precmd
  precmd_functions=(${(@)precmd_functions:#_self_destruct_hook})
  builtin unfunction _self_destruct_hook
  unset self_destruct_functions
}

precmd_functions=(_self_destruct_hook ${precmd_functions[@]})

# write current location
function update_cwd_file() {
  current_pid=$(echo $$)
  current-location write zsh $PWD $current_pid
}

add-zsh-hook -Uz chpwd update_cwd_file

# chpwd hook is not triggered on startup by design so we trigger it once manually
self_destruct_functions=(${self_destruct_functions[@]} update_cwd_file)
```

## Nix

Nix and NixOS users may try this tool without installing it:

``` sh
nix run github:aitvann/current-location -- --help
```
