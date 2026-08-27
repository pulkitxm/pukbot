# Installation

Install the [Pukbot GitHub App](https://github.com/apps/pukbot) on the
repositories where operations may run. Select only the repositories that need
access, and approve Actions write permission when workflow dispatches are
required.

Linux and macOS:

    curl --proto '=https' --tlsv1.2 -fsSL https://gitbot.pulkit.page/install.sh | sh

Windows PowerShell:

    irm https://gitbot.pulkit.page/install.ps1 | iex

Cargo:

    cargo install pukbot --locked

Verify the installation and install completions for the current shell:

    pukbot --version
    pukbot completions --install

Check for a release update without replacing the executable:

    pukbot update --check
