# Installation

Install the [Gitbot GitHub App](https://github.com/apps/pulkit-gitbot) on the
repositories where comments may be posted. Select only the repositories that
need access.

Linux and macOS:

    curl --proto '=https' --tlsv1.2 -fsSL https://gitbot.pulkit.page/install.sh | sh

Windows PowerShell:

    irm https://gitbot.pulkit.page/install.ps1 | iex

Cargo:

    cargo install gitbot --locked
