import { useEffect, useRef, useState } from "react";
import { Mic, Square, TextCursorInput, X } from "lucide-react";
import { Button } from "@/components/ui/button";

const MAX_MS = 60_000;

export function VoiceInput({
  onTranscript,
}: {
  onTranscript: (text: string) => void;
  tenderId?: string;
}) {
  const [listening, setListening] = useState(false);
  const [denied, setDenied] = useState(false);
  const streamRef = useRef<MediaStream | null>(null);
  const recorderRef = useRef<MediaRecorder | null>(null);
  const chunksRef = useRef<Blob[]>([]);
  const timerRef = useRef<number | null>(null);

  function stopTracks() {
    streamRef.current?.getTracks().forEach((track) => track.stop());
    streamRef.current = null;
    recorderRef.current = null;
  }

  function clearTimer() {
    if (timerRef.current != null) {
      window.clearTimeout(timerRef.current);
      timerRef.current = null;
    }
  }

  async function start() {
    setDenied(false);
    if (!navigator.mediaDevices?.getUserMedia) {
      setDenied(true);
      return;
    }
    try {
      const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
      streamRef.current = stream;
      chunksRef.current = [];
      if (typeof MediaRecorder !== "undefined") {
        const recorder = new MediaRecorder(stream);
        recorderRef.current = recorder;
        recorder.ondataavailable = (event) => {
          if (event.data.size) chunksRef.current.push(event.data);
        };
        recorder.start();
      }
      setListening(true);
      timerRef.current = window.setTimeout(() => {
        void stop();
      }, MAX_MS);
    } catch {
      setDenied(true);
      stopTracks();
    }
  }

  async function stop() {
    clearTimer();
    const recorder = recorderRef.current;
    if (recorder && recorder.state !== "inactive") recorder.stop();
    stopTracks();
    setListening(false);
  }

  function cancel() {
    clearTimer();
    chunksRef.current = [];
    if (recorderRef.current && recorderRef.current.state !== "inactive") {
      recorderRef.current.stop();
    }
    stopTracks();
    setListening(false);
  }

  useEffect(
    () => () => {
      cancel();
    },
    [],
  );

  return (
    <div className="flex flex-col gap-3">
      <div className="flex flex-wrap items-center gap-2">
        <Button
          type="button"
          size="sm"
          variant={listening ? "destructive" : "outline"}
          onClick={listening ? () => void stop() : () => void start()}
        >
          {listening ? (
            <Square data-icon="inline-start" />
          ) : (
            <Mic data-icon="inline-start" />
          )}
          {listening ? "Stop recording" : "Push to talk"}
        </Button>
        <Button type="button" size="sm" variant="ghost" onClick={cancel}>
          <X data-icon="inline-start" />
          Cancel
        </Button>
        <Button
          type="button"
          size="sm"
          variant="ghost"
          onClick={() => onTranscript("")}
        >
          <TextCursorInput data-icon="inline-start" />
          Insert transcript
        </Button>
      </div>
      {listening ? (
        <p role="status" className="flex items-center gap-2 text-sm">
          <span
            aria-hidden="true"
            className="size-2 animate-pulse rounded-full bg-destructive"
          />
          Recording. Maximum 60 seconds.
        </p>
      ) : null}
      {denied ? (
        <p className="text-sm text-destructive">
          Microphone permission was denied. Nothing was uploaded.
        </p>
      ) : null}
      <p className="text-xs text-muted-foreground">
        Transcripts stay in the unsent draft. They do not approve work.
      </p>
    </div>
  );
}
