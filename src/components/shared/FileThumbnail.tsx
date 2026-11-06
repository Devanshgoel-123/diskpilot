import { File } from "lucide-react";
import { useEffect, useRef, useState } from "react";

import { cachedThumbnail, loadThumbnail } from "@/lib/thumbnails";
import { cn } from "@/lib/utils";

/** A file's Quick Look thumbnail, loaded when it scrolls into view. */
export function FileThumbnail({
  path,
  size = 32,
  className,
}: {
  path: string;
  size?: number;
  className?: string;
}) {
  const ref = useRef<HTMLSpanElement>(null);
  const [loaded, setLoaded] = useState<{ key: string; url: string | null } | null>(null);
  const key = `${path}@${size}`;
  const cached = cachedThumbnail(path, size);
  const url = cached !== undefined ? cached : loaded?.key === key ? loaded.url : undefined;

  useEffect(() => {
    if (cachedThumbnail(path, size) !== undefined) return;
    const el = ref.current;
    if (!el) return;
    let live = true;
    const io = new IntersectionObserver(
      (entries) => {
        if (!entries.some((e) => e.isIntersecting)) return;
        io.disconnect();
        void loadThumbnail(path, size).then((u) => live && setLoaded({ key, url: u }));
      },
      { rootMargin: "200px" },
    );
    io.observe(el);
    return () => {
      live = false;
      io.disconnect();
    };
  }, [key, path, size]);

  return (
    <span
      ref={ref}
      aria-hidden
      className={cn(
        "flex shrink-0 items-center justify-center overflow-hidden rounded-[4px] bg-track",
        className,
      )}
      style={{ width: size, height: size }}
    >
      {url ? (
        <img src={url} alt="" className="size-full object-cover" draggable={false} />
      ) : url === null ? (
        <File className="size-1/2 text-subtle-foreground" />
      ) : null}
    </span>
  );
}
