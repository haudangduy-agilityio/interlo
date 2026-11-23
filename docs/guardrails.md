# Guardrails

Rules for every change. A change that breaks one is not done.

1. **Privacy.** Nothing leaves the Mac during a meeting. The only network use is
   a model download or a driver installer download that the owner starts, and
   the update check outside meetings (D-61, D-69). Every outbound call goes
   through the policy gate in `crates/meeting-core/src/policy.rs`. No cloud
   engine in v1 (D-42). The policy's model-host allow list (`allow_model_host`,
   `revoke_model_host`, `check_model_download`) permits only a model download
   from an allowed host after the owner's action. The update host and the driver
   host are also on the allow list. The update check sends only the request for
   the manifest file. The allow list never grants text or audio.
2. **Text only.** Never store audio. Never rewrite a final transcript line.
3. **No other product names** in code, comments, strings, tests, sample data, docs,
   commit messages or UI text. Use generic terms: "meeting app", "virtual
   microphone", "virtual audio device".
   The README and every other text in the repository name only Interlo.
   Libraries and models are named only in `docs/tech-stack.md`, in the notice that
   is generated at release and shipped inside the app, and in the licence files
   beside bundled fonts, as dependency attribution (D-56, D-192).
4. **English** in code, comments, identifiers, docs and UI text. Vietnamese UI text
   comes from the interface-language setting, never from hard-coded strings.
5. **Secrets.** API keys live in the macOS Keychain only. They never reach the
   webview, a log, a file or a test fixture.
6. **Logs** carry timings, event names and error codes. Transcript text appears
   only when debug logs are switched on.
7. **Models** are never committed and never downloaded without the owner's click.
   A model that fails its checksum never runs.
8. **Real-time audio callbacks** never allocate, lock or log.
9. **Quick actions** never read a password field, log keystrokes or paste without
   the owner's confirm.
