import { Fragment } from "react";

/**
 * Short facts shown as "a · b · c". Each fact is isolated, so an Arabic unit or
 * file name keeps its own direction without reordering the facts around it.
 */
export function MetaParts({
  parts,
}: {
  parts: (string | null | undefined | false)[];
}) {
  const shown = parts.filter((part): part is string => Boolean(part));
  return (
    <>
      {shown.map((part, index) => (
        <Fragment key={index}>
          {index ? " · " : null}
          <bdi>{part}</bdi>
        </Fragment>
      ))}
    </>
  );
}
