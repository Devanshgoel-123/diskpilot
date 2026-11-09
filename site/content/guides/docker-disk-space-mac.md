---
title: How to free Docker disk space on a Mac
h1: How to free Docker disk space on a Mac
nav: Free Docker disk space
description: Where Docker Desktop and OrbStack keep their data on a Mac, which Docker commands free space safely, and what to leave alone.
summary: Find out what Docker is using, and free it without losing volumes.
order: 3
updated: 2026-11-09
---

Docker on a Mac runs inside a Linux virtual machine. Every image, container, volume and layer of build cache lives on that machine's disk, which macOS sees as one large file. That is why Docker often shows up as tens of gigabytes of System Data.

## Where Docker keeps its data

- **Docker Desktop** keeps its virtual disk in `~/Library/Containers/com.docker.docker`.
- **OrbStack** keeps its machines and containers in `~/.orbstack`.
- **Colima** keeps its VMs in `~/.colima`.

Never delete these files by hand. You would lose every volume, and volumes often hold databases and other data you cannot recreate.

## See what is using the space

Start with Docker's own summary:

```sh
docker system df
```

It lists images, containers, local volumes and build cache, with how much of each is reclaimable.

## Free space safely

Remove build cache, which Docker recreates when you build again:

```sh
docker builder prune
```

Remove images that no container uses (add `-a` to include every image without a running container):

```sh
docker image prune
```

Remove stopped containers:

```sh
docker container prune
```

Be careful with volumes. `docker volume prune` deletes every volume no container is using right now, including the database of a project you stopped last week. Remove volumes one at a time, by name, once you know what is in them.

## Shrinking the disk image

Removing data inside Docker frees space in its virtual disk. Docker Desktop also lets you set a maximum size in **Settings > Resources**. Lowering it recreates the disk, which deletes everything inside, so only do it after the steps above.

## With DiskPilot

DiskPilot's **Docker** page lists images, containers, volumes and build cache with their sizes, and runs the same Docker commands for you after you review them:

- Stop or remove a container in one click. When you remove one, DiskPilot lists the volumes it uses; tick only those you also want deleted, then type the container's name to confirm. Volumes another container uses are kept.
- Delete a volume only when no container uses it, after typing its name.
- Remove images and clear build cache.

If Docker changed since you reviewed, DiskPilot asks you to review again instead of running something different. It never touches Docker's files directly.
