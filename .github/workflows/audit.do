exec >&2
redo-ifchange *.yml

engine=docker
image=ghcr.io/zizmorcore/zizmor:latest

$engine pull $image
docker run --rm -v .:/workflows -w /workflows $image *.yml
