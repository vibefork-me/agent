# Bootstrap installers

`generate-installers.py` renders the Windows and Unix bootstrap scripts from a release directory. The generated scripts contain SHA-256 pins for the included platform binaries and fetch only those pinned assets. The published installer script and its embedded pins are delivered over HTTPS; the script never executes an asset until its bytes match the embedded pin. The scripts use PowerShell 5.1 built-in hashing on Windows and `sha256sum` or `shasum` on Unix. The installed agent continues to verify signed manifests for later updates.
