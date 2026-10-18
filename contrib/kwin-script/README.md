# Tsuba focus reporter

A small KWin script that reports the focused window (process id, class, name, title) to
tsubad. tsubad loads it by itself while a linked game has "only while focused" switched on, so
installing it by hand is only needed to keep it loaded all the time:

    kpackagetool6 --type=KWin/Script --install contrib/kwin-script
    kwriteconfig6 --file kwinrc --group Plugins --key tsuba-focusEnabled true
    qdbus6 org.kde.KWin /KWin reconfigure
