# Clibby

![Clibby logo](assets/logo.png)

A command-line clipboard tool for files, directories, and imported text. File and directory entries are lightweight references to their original paths; `clb` does not recursively copy them into its own cache.

On Windows, `clb` also interoperates with the system file clipboard (`CF_HDROP`):

- `clb copy` publishes the copied file or directory to the Windows clipboard, so it can be pasted into Explorer and other file-aware applications.
- Before `paste`, `list`, `invoke`, or `clb` without a subcommand, `clb` records file and directory paths currently copied by Explorer or another Windows application. It does not recursively copy those paths, so listing a large directory remains fast.
- It also imports Unicode text copied with `Ctrl+C` from applications such as editors and browsers. Text is stored as a UTF-8 `clipboard.txt` snapshot and shown with a short preview in `list` output.
- Piping UTF-8 text to `clb` with no subcommand copies it to the Windows text clipboard (`CF_UNICODETEXT`) and records it in history.
- Raw image, HTML, and other non-file/non-text clipboard formats are intentionally left unchanged.

## QuickStart

### Install

```powershell
git clone git@github.com:gloridifice/clibby.git clibby
cd clibby
cargo install --path .
```

### Usage

```powershell
clb c hello.rs # copy a file (also works with folders)
cd D:\foo\     # enter anthor directory
clb p          # hello.rs will be pasted under foo
```

```powershell
cat "README.md" | clb # you can pipe text into clipboard by this
```


## Commands

```text
clb copy|c <PATH>
clb paste|p [TARGET] [--index|-i <INDEX>]
clb                     # Show the newest item, or copy piped UTF-8 text.
clb list|ls [--number|-n <NUMBER>]
clb clean               # Clean cache 
clb invoke|i [INDEX]    # Open file in clipboard by default software
```

`paste --index` also accepts `-n` as a compatibility alias. Index `0` always means the most recent history item.

## Examples

```powershell
# Record a file or directory reference. On Windows this also updates the system clipboard.
# No directory contents are copied into clb's cache.
clb c a.txt
clb copy .\assets

# Paste the newest entry into the current directory.
clb p

# A trailing slash or backslash means "paste into this directory".
# The result is foo\a.txt.
clb p foo\

# Without a trailing separator, foo is the exact destination name.
clb p foo

# Paste the third newest item.
clb p -n 2

# When the selected item is text, paste writes the exact text to standard output.
# This works with pipes; a destination path is invalid for text entries.
clb p | Set-Clipboard
clb p -i 2 | some-command

# Show eight recent items, the newest item, or show nothing.
clb ls
clb
clb ls -n 0

# Open the newest item, or item number two, with the system-default application.
clb i
clb i 2
```

For file and directory entries, `paste` copies the referenced source to the destination and refuses to overwrite an existing destination. Parent directories are created when necessary. A file or directory reference cannot be pasted or opened after its original source has been moved or deleted.

For text entries, `paste` writes the cached UTF-8 text directly to standard output, without adding a status message or newline, so it can be piped to another command. Do not provide `TARGET` when pasting text; text entries cannot be written to an output path.

`clb` without a subcommand synchronizes the system clipboard and displays the newest history item. When standard input is redirected or piped, such as `cat README.md | clb`, it instead reads UTF-8 text, saves it in history, and copies it to the Windows text clipboard. Each list row is formatted as `[index][type] content`: file entries use their extension as `type` (or `file` when extensionless), directories use `directory`, and text entries use `text` with a 100-character preview.

## Clean history and cache

```powershell
clb clean
```

`clean` removes all `clb` history entries and cached imported text. It does **not** modify the Windows system clipboard. Running `clb ls` afterward will record whichever file path or text is still in the system clipboard.

## Data directory

History metadata and cached imported text are stored in:

- Windows: `%LOCALAPPDATA%\clipboarry`
- macOS: `~/Library/Application Support/clipboarry`
- Linux and other Unix systems: `$XDG_DATA_HOME/clipboarry`, or `~/.local/share/clipboarry`

Set `CLB_DATA_DIR` to use another data directory. This is useful for testing or keeping separate histories:

```powershell
$env:CLB_DATA_DIR = "D:\clb-data"
clb ls
```

## Platform support

File-reference history and default-application opening work on Windows, macOS, and Linux. Windows provides the system clipboard bridge. On macOS and Linux, `clb` currently keeps history only inside its own data directory.
