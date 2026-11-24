# NEXUS contributor installation and setup

Start with Git and Node 22 (24 also accepted). The canonical guide is [CONTRIBUTING.md](../../CONTRIBUTING.md).

- `node nexus.mjs doctor` (alias `check`) reports prerequisites without changing settings, probing accounts or installing tools.
- `node nexus.mjs setup` prepares native tools, Python 3.12 in `.venv`, frontend dependencies and runtime assets. It does not compile or launch NEXUS.
- `node nexus.mjs install` prepares the environment and registers a user-local command. Add its printed directory to user PATH if needed. No implicit build or launch.
- `node nexus.mjs build` performs the same preparation and then compiles a contributor build.

Required system changes are displayed and need interactive approval or `--yes`. Refused/failed installation gives an error and can be retried. Existing incompatible Python environments are preserved, not deleted. Training data, model training, Worker deployment and personal accounts are outside setup.

Default contributor identity is `com.nexus.assistant.contributor`; explicit `--profile personal` retains the installed personal identity. Node is required before any JavaScript setup command can run. A fresh clone does not automatically add `nexus` to PATH.

Implementation, tests, platform limitations and pending device proofs: [change record](../changes/2026-11-05-contributor-setup.md).
