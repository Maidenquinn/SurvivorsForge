# Runtime check on Arch Linux, the base of SteamOS: only what a desktop already has.
# It needs at least one system font: Slint (fontique) panics with `NoMatch` without any.
FROM archlinux:latest

RUN pacman -Syu --noconfirm --needed \
        fontconfig libxkbcommon libxkbcommon-x11 wayland libx11 libxcursor libxrandr libxi \
        mesa libglvnd fuse2 file ttf-dejavu \
    && pacman -Scc --noconfirm

RUN useradd -m deck
USER deck
WORKDIR /home/deck
