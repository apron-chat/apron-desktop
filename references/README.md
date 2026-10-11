# Reference review

Implementation targets Apron protocol 8. Sources inspected on 2026-10-09:

- https://github.com/shazow/apron — current HEAD `3df659ac821ec67554fc0cdbf698842758d15928`.
  `PROTOCOL.md` is included here for offline review under its upstream license.
- https://github.com/apron-chat/apron-web — current HEAD `8efacfbe241134bd414bad81c2d49ec3e7683913`.
  Reviewed README, architecture, protocol client, reducer, and unread handling.
- Saved web checkout `/workspace/apron-web` at `fbb3e71a3483544a4bdf7236070cf48e87686377`.
  Read only; its main branch and working tree were not changed.
- Reviewed protocol `AGENTS.md` in the saved checkout, and Go server DEVELOPMENT.md.
  Workspace `.agents` was empty; the current web checkout contained no AGENTS.md or SKILL.md.

The current web reference checkout is excluded from the standalone distribution.
The native implementation does not copy web-client code.
