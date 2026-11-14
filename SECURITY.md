# Security policy

## Reporting a vulnerability

Please report a vulnerability in claude-here (the binaries, the image files under `images/`,
the install script and the Homebrew formula) privately, through GitHub's
[private vulnerability reporting](https://github.com/paxel/claude-here/security/advisories/new)
(the repository's **Security** tab, **Report a vulnerability**). Do not open a public issue,
pull request or discussion for it.

Include what is affected (version, command or toolchain), how to reproduce it, and what an
attacker gains, for example an escape from the sandbox or a restriction mode that can be
lifted from inside. The report and its fix are discussed in that advisory, which stays
private until a release with the fix is out.

## What is already checked

Every pull request and every change to the main branch runs CodeQL and a dependency audit,
and SonarQube Cloud and OpenSSF Scorecard analyse the main branch. The CodeQL and Scorecard
findings are the repository's code scanning alerts, SonarQube Cloud's are on its project page;
a report does not need to repeat them.
