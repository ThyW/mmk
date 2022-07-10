# About
`mmk` or `mimic` is a tool which allows one to change a keyboard layout for a single X window while having a different layout for the rest of their system.

# Installation
`mmk` uses both `xcb` and `Xlib`, so make sure you have them installed for your system.
```console
$ cargo build --release
$ ln -s target/release/mmk somewhere/in/your/PATH
```

# Usage
First, set two layouts you want to use using `setxkbmap`:
```console
$ setxkbmap -layout dvorak,us
```

Your main system layout is now Dvorak and your backup layout is the US layout. After this is done, run `mmk` on a window of your choice:
```console
$ mmk -w 123456
```

If everything works correctly, the specified window should use the specified second layout, US in our case.
