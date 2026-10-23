# Generic login failure text

The original December 2011 client stores login-error wording in its executable. The `loginerror` recipe transformation changes its bad-credentials message to **Incorrect username or password.** The account server must return the same bad-credentials result for missing accounts and incorrect passwords.

The transformation accepts only the verified original client SHA-256 `af1d0fa718f55a17ea8c05632bb92ca71c0ff003daed4e0901a128c546910991`. It uses 32 bytes of existing `.rdata` padding for the message, extends that section's virtual size within its existing raw size, and updates the relocated message pointer for client failure code 4. It changes no game code, file size, other message pointers, or asset offsets. The resulting SHA-256 must be `5774e3bde5761086075a443b07eab89f2749c99156af38bcbf358c891bca58a3`.

Recipe `0.27-r2` requires launcher 1.0.1 and retains every other file fingerprint and original download source from `0.27-r1`. Original game executables are not hosted or committed. Both fresh installation and repair reconstruct the same corrected bytes from the verified original installer.

To generate the recipe, pass `--generic-login-errors --min-launcher 1.0.1 --game 0.27-r2` to `recipe-tool build` with the existing source specification and extracted installer. `recipe-tool patch-login-message --file <original-client> --out <corrected-client>` performs the same guarded transformation for local verification.

The active server-info remains on the previous recipe until a compatible launcher release is available; adding the new recipe alone does not switch existing clients.
