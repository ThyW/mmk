# About
`mmk` or `mimic` is a tool which allows one to change a keyboard layout for a single X window while having a different layout for the rest of their system.

# Installation
`mmk` uses both `xcb` and `Xlib`, so make sure you have them installed for your system.
```console
$ cargo build --release
$ ln -s target/release/mmk somewhere/in/your/PATH
```

# Usage
First, set two or more layouts you want to use using `setxkbmap`, for example:
```console
$ setxkbmap -layout dvorak,us,de -variant ,,koy
```

Your main system layout is now Dvorak and your backup layouts are US and German with the `k.o,y` variant. After this is done, run `mmk` on a window of your choice, for example Discord:
```bash
$ mmk --class discord.discord --layout 1 --all
# --layout specifies which layout to use, 0 meaning the first, 1 second and so on... 
# --all tells mimic to run on all windows which fit the specified criteria
```

The window should now register the specified layout.

# How it works
