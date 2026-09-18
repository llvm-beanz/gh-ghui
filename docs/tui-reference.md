# TUI reference

Launch the terminal interface with an optional GitHub project URL or session-state
file:

```sh
gh ghui tui [URL_OR_PATH]
```

A GitHub Projects URL opens that project. An existing path loads a saved session.
A path that does not exist becomes the destination used by `:w` and `:wq`.

## Global shortcuts

These shortcuts work in every mode.

| Shortcut | Action |
| --- | --- |
| `Ctrl+C` | Quit immediately |
| `Ctrl+R` | Refresh the active tab's project from GitHub |
| `Ctrl+Tab` | Switch to the next tab |
| `Ctrl+Shift+Tab` | Switch to the previous tab |

## Error dialogs

Recoverable TUI errors, including authentication, GitHub API, validation, and
session-file failures, appear in a modal dialog instead of being written over
the terminal interface. Press `Enter` or `Escape` to dismiss the dialog.

`Ctrl+C` remains available while an error dialog is open. Other shortcuts are
blocked until the dialog is dismissed.

## Normal mode

Normal mode navigates project items and enters other modes.

| Shortcut | Action |
| --- | --- |
| `j` or `Down` | Select the next item |
| `k` or `Up` | Select the previous item |
| `Page Down` | Move down 10 items |
| `Page Up` | Move up 10 items |
| `g` or `Home` | Select the first item |
| `G` or `End` | Select the last item |
| `Enter` | Activate the selected row and its first visible editable field |
| `Space` | Open the selected pull request in `tuicr` |
| `?` | Open the item status emoji legend |
| `:` | Enter command mode |

Opening a pull request temporarily leaves `ghui` while `tuicr` runs. Exiting
`tuicr` returns to the current project view. The `tuicr` executable must be
available on `PATH`.

## Item status

The generated `State` column summarizes each item's type and state. It is
available in both `gh ghui view` and the TUI and can be hidden from the TUI's
columns menu.

| State | Emoji |
| --- | --- |
| Open PR | 🟢 |
| Closed PR | 🛑 |
| Merged PR | 🏁 |
| Open Issue | ⚠️ |
| Fixed Issue | ✅ |
| In Progress Issue | 🏃 |
| Duplicate Issue | ❓ |
| Closed Issue | ❌ |

Issue classification uses the project `Status` and `Labels` values when they
identify In Progress or Duplicate items. Closed-as-completed issues and issues
with a Fixed, Done, or Completed status are treated as fixed; other closed
issues use the closed marker.

## Active row mode

The active cell is highlighted within the selected row. Only visible editable
project fields participate in field navigation.

| Shortcut | Action |
| --- | --- |
| `Tab` | Activate the next editable field, wrapping at the end |
| `Shift+Tab` | Activate the previous editable field, wrapping at the start |
| `j` or `Down` | Move to the next item and keep the field active |
| `k` or `Up` | Move to the previous item and keep the field active |
| `Enter` | Edit the active field |
| `Escape` | Return to normal mode |

## Field editor

The editor shown for a field depends on its GitHub Projects field type.

| Field type | Editor | Accepted value |
| --- | --- | --- |
| Text | Text input | Free-form text |
| Number | Text input | A finite number |
| Date | Text input | A valid calendar date in `YYYY-MM-DD` format |
| Single select | Selection list | One configured field option |
| Iteration | Selection list | One configured iteration |

### Text, number, and date input

| Shortcut | Action |
| --- | --- |
| Character keys | Insert text at the end of the value |
| `Backspace` | Delete the last character |
| `Enter` | Validate and save the value |
| `Escape` | Cancel and return to active row mode |

Validation errors appear in an error dialog and leave the editor open. Dismiss
the dialog to correct the value.

### Selection lists

| Shortcut | Action |
| --- | --- |
| `j` or `Down` | Select the next option |
| `k` or `Up` | Select the previous option |
| `g` or `Home` | Select the first option |
| `G` or `End` | Select the last option |
| `Enter` | Save the selected option |
| `Escape` | Cancel and return to active row mode |

## Columns menu

Open the columns menu with `:columns`. Built-in fields and project-specific
fields are listed with checkboxes. Column choices apply only to the active tab.

| Shortcut | Action |
| --- | --- |
| `j` or `Down` | Select the next column |
| `k` or `Up` | Select the previous column |
| `g` or `Home` | Select the first column |
| `G` or `End` | Select the last column |
| `Space` | Show or hide the selected column |
| `Enter` or `Escape` | Close the menu |

## Command mode

Press `:` in normal mode, type a command without the leading colon, and press
`Enter`. Press `Escape` to cancel command entry.

| Shortcut | Action |
| --- | --- |
| `Up` | Recall the previous command |
| `Down` | Recall the next command, or restore the command being typed |
| `Backspace` | Delete the last character |
| `Enter` | Execute the command |
| `Escape` | Cancel command entry |

| Command | Action |
| --- | --- |
| `:q` | Quit without saving session state |
| `:w` | Save the complete session to its configured state path |
| `:wq` | Save and quit; remain open if saving fails |
| `:e URL` | Open a GitHub project in the active tab |
| `:e PATH` | Load a session from an existing path, or configure a missing path for future saves |
| `:d` | Remove the selected item from the project |
| `:dCOUNT` | Remove the selected item and following visible items, up to `COUNT` total |
| `:columns` | Open the columns menu for the active tab |
| `:refresh` | Refresh the active tab's project from GitHub |
| `:legend` or `:emoji` | Open the item status emoji legend |
| `:filter EXPRESSION` | Apply a GitHub Projects-style filter to the active tab |
| `:filter` | Clear the active tab's filter |
| `:sort FIELD[:asc\|desc]` | Sort the active tab by a field (ascending by default) |
| `:sort` | Clear the active tab's sort |
| `:tabnew` | Duplicate the active project and view in a new tab |
| `:tabnew URL` | Open a GitHub project in a new tab |
| `:tabnext` or `:tabn` | Switch to the next tab |
| `:tabprevious` or `:tabp` | Switch to the previous tab |
| `:tabclose` or `:tabc` | Close the active tab |

With an active filter or sort, `:dCOUNT` follows the visible row order.
Removing an item affects only the project; it does not delete the underlying
issue or pull request.

Closing the only tab resets it to an empty tab rather than closing the TUI.

### Filter expressions

Filters support field values (`status:done`), quoted values
(`status:"In Progress"`), comma-separated alternatives (`label:bug,docs`),
negation (`-label:duplicate`), `has:`, `no:`, `is:`, `reason:`, general text,
`*` wildcards, comparisons (`points:>=3`), and inclusive ranges
(`points:1..5`). Multiple clauses are combined with AND. Relative keywords
such as `@me`, `@today`, and `@current` are not currently supported.

Sort specifications use a field name with an optional `:asc` or `:desc`.
Missing values sort after populated values. Filters and sorting apply only to
the active tab and are included in saved sessions.

### Command history

Executed non-empty commands are saved immediately and remain available across
TUI sessions. Consecutive duplicate commands are stored once, and the latest
1,000 commands are retained. The history file defaults to:

- Windows: `%APPDATA%\ghui\history.json`
- macOS: `~/Library/Application Support/ghui/history.json`
- Linux and other Unix systems: `$XDG_STATE_HOME/ghui/history.json`, or
	`~/.local/state/ghui/history.json` when `XDG_STATE_HOME` is unset

Set `GHUI_HISTORY_FILE` to use a different history file.

## Tabs and saved sessions

Each tab has independent selected-row and visible-column state. `:tabnew`
creates another view of the same fetched project, while `:tabnew URL` allows
multiple projects to be open together.

A saved session is readable JSON containing:

- Every open tab, in tab-bar order
- Each tab's project URL
- Each tab's selected row
- Each tab's visible columns
- The active tab

Fetched project data is not saved. Loading a session fetches each project again
from GitHub so the displayed data is current.

Use `:refresh` or `Ctrl+R` to fetch the active tab's project again without
changing its visible columns. The selected row is retained when possible and
clamped if the refreshed project has fewer items.
