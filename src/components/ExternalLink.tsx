import { useState, type MouseEvent, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import { nativeDesktop } from "../api";

export function webLink(value?: string | null): string | null {
  if (!value || value.length > 8192 || /[\u0000-\u001f\u007f]/.test(value))
    return null;
  try {
    const url = new URL(value);
    return ["http:", "https:"].includes(url.protocol) &&
      !url.username &&
      !url.password
      ? url.href
      : null;
  } catch {
    return null;
  }
}

export function ExternalLink({
  href,
  children,
  className,
  title,
}: {
  href?: string | null;
  children?: ReactNode;
  className?: string;
  title?: string;
}) {
  const destination = webLink(href);
  const [failed, setFailed] = useState<string | null>(null);
  async function open(event: MouseEvent<HTMLAnchorElement>) {
    if (
      !nativeDesktop() ||
      !destination ||
      (event.type === "auxclick" && event.button !== 1)
    )
      return;
    event.preventDefault();
    setFailed(null);
    try {
      await invoke("open_external_url", { url: destination });
    } catch {
      setFailed(destination);
    }
  }
  if (!destination) return <span className={className}>{children}</span>;
  return (
    <>
      <a
        href={destination}
        className={className}
        title={title}
        target="_blank"
        rel="noopener noreferrer"
        onClick={open}
        onAuxClick={open}
      >
        {children}
      </a>
      {failed === destination ? (
        <span role="alert" className="field-help">
          {" "}
          Browser could not open. Copy this address into your browser:{" "}
          {destination}
        </span>
      ) : null}
    </>
  );
}
