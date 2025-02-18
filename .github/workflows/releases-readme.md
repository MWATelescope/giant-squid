# Readme (Releases)

This is a release of `giant-squid`, an alternative MWA ASVO client, obtained
from the [GitHub releases
page](https://github.com/MWATelescope/giant-squid/releases).

## Licensing

`giant-squid` is licensed under the [Mozilla Public License 2.0 (MPL
2.0)](https://www.mozilla.org/en-US/MPL/2.0/). The LICENSE file is the relevant
copy.

## What are these different x86-64 versions?

They are [microarchitecture
levels](https://en.wikipedia.org/wiki/X86-64#Microarchitecture_levels). By
default, Rust compiles for all x86-64 CPUs; this allows maximum compatibility,
but potentially limits the runtime performance because many modern CPU features
can't be used. Compiling at different levels allows the code to be optimised for
different classes of CPUs so users can get something that works best for them.

### Rule of thumb for which microarchitecture to use

You're probably safe with x86-64-v2, especially if your CPU was made after 2010.
If your CPU is older than that, you may want x86-64. If your CPU is a little
newer, x86-64-v3 is also likely to work. x86-64-v4 isn't widely supported
(AVX-512), so only use that if you know it works.
