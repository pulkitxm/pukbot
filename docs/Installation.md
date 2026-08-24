# Installation

Install the [Pukbot GitHub App](https://github.com/apps/pulkit-pukbot) on the
repositories where comments may be posted. Select only the repositories that
need access.

Linux and macOS:

    curl --proto '=https' --tlsv1.2 -fsSL https://pukbot.pulkit.page/install.sh | sh

Windows PowerShell:

    irm https://pukbot.pulkit.page/install.ps1 | iex

Cargo:

    cargo install pukbot --locked

Verify the installation and install completions for the current shell:

    pukbot --version
    pukbot completions --install

Check for a release update without replacing the executable:

    pukbot update --check
