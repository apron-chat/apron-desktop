# Native passkey integration decision

Passkeys are not implemented. No authenticator or real authentication service was
installed or used. Current reference: protocol §4.10 and Go server `0670838`
`internal/server/webauthn.go`. Email/token support is independent of this work.

## Trust boundary

The server validates the WebSocket Origin against its configured RP origins and
binds a pending ceremony and resume token to that origin. A desktop application's
ability to construct an Origin header is not evidence that it controls that web
origin. A CTAP library alone does not provide browser origin isolation. Simply
claiming the web client's HTTPS origin would cross this boundary incorrectly.

## Preferred cooperating browser handoff

A server-owned HTTPS page performs WebAuthn in its real browser origin, using the
server's existing RP ID and allowed-origin policy. The desktop client starts a
fresh handoff with random state, a PKCE S256 challenge and a short expiration. A
user-visible browser confirmation names the desktop client and destination server.
After authentication, the server issues a single-use code bound to the challenge,
server, authenticated identity and exact registered redirect. The desktop exchanges
it with its verifier over TLS for an explicitly native-scoped session.

Use a loopback receiver bound before opening the browser, on a random loopback
port with an exact callback path, plus state verification and single-use expiry.
Do not put a bearer token in callback URLs, browser history, command arguments or
logs. Do not read browser cookies, storage or existing web tokens. Reject changed
origins/servers, unsolicited callbacks, duplicated codes and mismatched state.
Close/cancel must dispose of the receiver and invalidate outstanding state. Store a
returned token only through the existing opt-in Secret Service path.

**Blocker:** Apron's current protocol has no such native handoff/code exchange or
native token-origin contract. This requires an agreed server/frontend extension,
redirect policy and session-revocation behavior, followed by synthetic endpoint
fixtures and actual authenticator/browser testing. It cannot be completed solely
inside this client without inventing an unsupported authentication endpoint.

## Direct roaming authenticator alternative

A native CTAP2 adapter could handle a roaming authenticator with user presence and
user verification. It needs a separately documented server-approved native-client
origin/RP trust contract; it must not reuse the web origin merely to satisfy an
allowlist. Linux does not automatically associate this binary with a website's RP.
The contract must state what the server trusts about the native application and
how registration, credential discovery, verification and origin-bound token resume
work. Platform credential-provider support would be a separate adapter.

Only after that decision: implement bounded challenge/credential parsing,
base64url validation, allow/exclude credential lists, discoverable credentials,
RP-ID checks, UV/UP enforcement, cancellation/timeouts, and exact request/response
binding. Keep one unauthenticated connection for each complete ceremony. Never
silently downgrade verification or auto-retry a possibly completed registration.
Test origin/RP mismatch, credential mismatch, expired challenges, denied UV,
cancellation, disconnect and replay before an actual opt-in authenticator test.

**Blocker:** no native-origin trust contract or selected authenticator adapter has
been approved. Hardware interaction and real-service testing are outside this
milestone. The choice of a browser handoff versus explicitly trusted native CTAP
must be made before implementing this authentication path.
