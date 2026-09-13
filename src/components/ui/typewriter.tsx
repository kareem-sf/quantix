/**
 * Typewriter text adapted from Ali Imam UI
 * (https://aliimam.in/docs/components/typewriter). One timer drives typing,
 * pausing and deleting; reduced motion shows the first phrase without typing.
 * Screen readers get the full phrase list once instead of each keystroke.
 */
import { useEffect, useState } from "react";
import { useReducedMotion } from "motion/react";
import { cn } from "@/lib/utils";

type TypewriterProps = {
  words: string[];
  speed?: number;
  delayBetweenWords?: number;
  cursor?: boolean;
  className?: string;
};

export function Typewriter({
  words,
  speed = 55,
  delayBetweenWords = 2200,
  cursor = true,
  className,
}: TypewriterProps) {
  const reduce = useReducedMotion();
  const [wordIndex, setWordIndex] = useState(0);
  const [length, setLength] = useState(0);
  const [deleting, setDeleting] = useState(false);
  const word = words.length ? words[wordIndex % words.length] : "";

  useEffect(() => {
    if (reduce || !words.length) return;
    const complete = !deleting && length === word.length;
    const timeout = window.setTimeout(
      () => {
        if (!deleting) {
          if (length < word.length) setLength(length + 1);
          else setDeleting(true);
        } else if (length > 0) setLength(length - 1);
        else {
          setDeleting(false);
          setWordIndex((index) => (index + 1) % words.length);
        }
      },
      complete ? delayBetweenWords : deleting ? speed / 2 : speed,
    );
    return () => window.clearTimeout(timeout);
  }, [deleting, length, word, words.length, speed, delayBetweenWords, reduce]);

  return (
    <span className={cn("inline", className)}>
      <span aria-hidden="true">
        {reduce ? word : word.slice(0, length)}
        {cursor && !reduce ? (
          <span className="ms-0.5 inline-block w-[2px] animate-pulse bg-current align-[-0.1em]">
            &#8203;
          </span>
        ) : null}
      </span>
      <span className="sr-only">{words.join(", ")}</span>
    </span>
  );
}
