# gpui-pre-linux

crates.io's `gpui-pre-linux` 0.3.8 with one change for ELYTerminal, which takes it through `[patch.crates-io]`.

gpui's `ClipboardItem` holds text, an image and files together. On X11 the clipboard and the primary selection took only the text, and on Wayland they offered only text types and sent the text whatever type was asked for; reading took the text, else an image. kitty's clipboard protocol, OSC 5522, writes and reads them together. X11 now writes the text as `UTF8_STRING`, an image as its own type and files as `text/uri-list`, and reads each kind; Wayland offers each entry's types, sends each as asked, and reads each kind. One module turns paths into a URI list and back for both, the list Wayland's drags already sent.

Remove this fork once a gpui-pre release writes and reads a whole `ClipboardItem` on Linux.
