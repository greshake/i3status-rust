# Contributing to i3status-rust

i3status-rust development is largely community driven. We rely on volunteers to
propose and implement most new features or new blocks, and often encourage users
to get their feet wet with Rust to do so.

If you would like to propose a new feature or block, it's not a bad idea to
search for relevant issues or open a new one for discussion first. It can often
be helpful to look for up support (or potential criticism) before you embark on
potentially time-consuming coding.

Bug fixes (or typos!), on the other hand, do not require a corresponding issue
-- feel free to open a PR directly if you spot one.

## Problematic Contributions

We will *not* generally accept the following:

- **PRs that just update dependencies to their newest version.** See below.

## Dependencies

i3status-rust depends on many crates and a few system dependencies. These
dependencies have allowed us to add many features, but impose a burden on anyone
using this project, especially if built from source (which is currently the
norm). Keep this in mind when proposing new dependencies or updates to existing
ones.

Good reasons for updating an existing dependency include, in roughly this order:

1. A newer version fixes a bug we have encountered.

2. We need/want functionality introduced in a newer version. For example,
   "bumping `i3ipc` to 0.8.4 might make our focused window block work with
   Sway".

3. A newer version has substantial performance improvements relevant to this
   project; or

4. A newer version removes a transient dependency. For example, "updating
   `thread_local` to 3.6 would remove the `unreachable` crate".

We generally like to see the following when adding a new crate or system
dependency:

1. New dependencies are relatively "proven" crates; and

2. Heavier dependencies are introduced to provide core functionality that will
   benefit a large number of users.

For cases where this is not true, we will consider using Cargo's feature flags
to make the dependency optional, or offer suggestions on how to avoid using the
dependency.

To make downstream packaging easier, maintainers bump dependencies as part of
the release process.

## Code and Git Commits

If you are new to Rust, Cargo and the compiler will be your best friends...
eventually. In the meantime, you can ask for advice on idiomatic code and we
will do our best to help you out, time permitting.

We will generally squash PRs that have commits like "fix typo", "updates", etc.
To avoid this, make your commit messages clear and the commits themselves as
targeted as possible. Use `git rebase` if you need to do this after the fact.

Please format your code with `rustfmt` before submitting a PR.  The easiest way
to do this is by running `cargo fmt`.

## Maintainership

i3status-rust is currently maintained by Kai Greshake and Aaron Jacobs, neither
of whom have unlimited time to devote to the project. If you are interested in
taking on a more active community role -- e.g. triaging issues or reviewing pull
requests -- please feel free to reach out to us.

## License

This project is licensed under the terms of the GPLv3, and any contribution you
make will be understood to be the same. See the [LICENSE.md](LICENSE.md) file
for details on these terms.
