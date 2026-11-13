# Security policy

sfes parses untrusted input (ROMs, save states, movies, integration files) and must never panic, hang or misbehave on it (FND-09). Reports of crashes, hangs, memory-safety issues in the FFI layers, or supply-chain problems are welcome.

## Reporting a vulnerability

Please report privately through GitHub: **Security → Report a vulnerability** on [waymost/sfes](https://github.com/waymost/sfes/security/advisories/new). Do not open a public issue.

Include the sfes version or commit, the input that triggers the problem (or how to produce it, if it involves a commercial ROM, which must not be attached), and what happens.

You can expect an acknowledgement within a week. Fixes are released as patch versions and credited in the advisory unless you prefer otherwise.

## Supported versions

Before v1.0 only the latest release receives fixes.
