# Completions and manuals

Pukbot generates completions from the same Clap command tree used for parsing.
Supported shells are Bash, Zsh, Fish, Elvish, and PowerShell.

Print a completion script:

```bash
pukbot completions bash
pukbot completions zsh
pukbot completions fish
pukbot completions elvish
pukbot completions powershell
```

Install a script to the standard user directory for the detected shell:

```bash
pukbot completions --install
```

Pass a shell explicitly when detection is unsuitable:

```bash
pukbot completions zsh --install
```

The install command writes one file and never modifies a shell profile. Bash
and Fish discover their standard user completion directories automatically.
For Zsh, include `$HOME/.zfunc` in `fpath`. Elvish and PowerShell users can load
the reported path from their profile.

Use `--json` to receive the generated script or installed path as one object.

Pukbot also generates roff manual pages from the command tree:

```bash
pukbot man
pukbot man --dir ./man
pukbot man --dir ./man --json
```

The directory form writes one page for the root command and every nested
subcommand. Release artifacts include the root manual and all five completion
scripts.
