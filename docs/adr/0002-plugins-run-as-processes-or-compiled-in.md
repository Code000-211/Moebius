# Plugins run as their own process or compiled into Moebius, behind one API

Moebius is a lean core that anyone can shape to their needs, as Pi and Radiance are: nearly everything beyond the core is a
plugin, including the ones Moebius ships (the GitHub source, the Pi-Bolt harness, the Retro), which are built and loaded the
same way a third party's are. A plugin is written against one API and used in either of two ways: as its own process that
speaks JSON Lines over stdin and stdout, in any language, isolated from Moebius; or compiled into the Moebius binary, the way
`pi-bolt compile` compiles Pi extensions in, so that calls into it cost nothing extra.

## Considered Options

- Native shared libraries loaded in process, as Radiance's `.so` plugins: fastest, but a faulty plugin takes Moebius down
  with it, and a stable ABI has to be kept by hand.
- WebAssembly: sandboxed and portable, but limited in what a plugin may reach (processes, files, network) and more work for
  plugin authors.
