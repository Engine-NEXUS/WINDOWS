import { useState, useRef, useCallback, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ENROLLMENT_PHRASES, enrollmentDelay, withEnrollmentMicrophone } from "./enrollmentSession";

/**
 * Voice Enrollment component for the NEXUS setup page.
 *
 * Records five enrollment clips and five held-out calibration clips.
 *
 * The voice profile is stored locally and never leaves the device.
 * Voice Lock remains off until the user enables it after enrollment.
 */

interface VoiceProfileStatus {
  enrolled: boolean;
  num_clips: number;
  threshold: number;
  created_at: number;
  updated_at: number;
}

const NUM_CLIPS = ENROLLMENT_PHRASES.length;
const CLIP_DURATION_MS = 3000; // 3 seconds per clip

export function VoiceEnrollment({ onStatusChange }: { onStatusChange?: (enrolled: boolean) => void } = {}) {
  const [status, setStatus] = useState<VoiceProfileStatus | null>(null);
  const [enrolling, setEnrolling] = useState(false);
  const [currentClip, setCurrentClip] = useState(0);
  const [recording, setRecording] = useState(false);
  const [countdown, setCountdown] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const [success, setSuccess] = useState<string | null>(null);

  const mediaStreamRef = useRef<MediaStream | null>(null);
  const audioCtxRef = useRef<AudioContext | null>(null);
  const recorderRef = useRef<ScriptProcessorNode | null>(null);
  const enrollmentRef = useRef<AbortController | null>(null);
  const mountedRef = useRef(true);

  // Load status on mount
  const refreshStatus = useCallback(async () => {
    try {
      const s = await invoke<VoiceProfileStatus>("get_voice_profile_status");
      setStatus(s);
      onStatusChange?.(s.enrolled);
    } catch (err) {
      setError(`Could not read voice profile: ${String(err)}`);
      onStatusChange?.(false);
    }
  }, [onStatusChange]);

  useEffect(() => {
    refreshStatus();
  }, [refreshStatus]);

  // Get microphone stream at 16kHz mono
  const getStream = useCallback(async (signal: AbortSignal): Promise<MediaStream> => {
    if (mediaStreamRef.current) return mediaStreamRef.current;
    const stream = await navigator.mediaDevices.getUserMedia({
      audio: {
        channelCount: 1,
        sampleRate: 16000,
        echoCancellation: true,
        noiseSuppression: true,
      },
    });
    if (signal.aborted) {
      stream.getTracks().forEach((track) => track.stop());
      signal.throwIfAborted();
    }
    mediaStreamRef.current = stream;
    return stream;
  }, []);

  // Record a single clip
  const recordClip = useCallback(async (signal: AbortSignal): Promise<Float32Array> => {
    const stream = await getStream(signal);
    signal.throwIfAborted();
    const audioCtx = new AudioContext({ sampleRate: 16000, latencyHint: "interactive" });
    audioCtxRef.current = audioCtx;

    const source = audioCtx.createMediaStreamSource(stream);

    // Use ScriptProcessor for simplicity (works everywhere)
    const processor = audioCtx.createScriptProcessor(4096, 1, 1);
    recorderRef.current = processor;

    const chunks: Float32Array[] = [];

    processor.onaudioprocess = (e) => {
      const input = e.inputBuffer.getChannelData(0);
      // Copy the data (the buffer is reused)
      chunks.push(new Float32Array(input));
    };

    source.connect(processor);
    processor.connect(audioCtx.destination);

    try {
      await enrollmentDelay(CLIP_DURATION_MS, signal);
    } finally {
      processor.onaudioprocess = null;
      processor.disconnect();
      source.disconnect();
      if (audioCtx.state !== "closed") await audioCtx.close();
      if (audioCtxRef.current === audioCtx) audioCtxRef.current = null;
      recorderRef.current = null;
    }

    // Concatenate all chunks
    const totalLen = chunks.reduce((sum, c) => sum + c.length, 0);
    const result = new Float32Array(totalLen);
    let offset = 0;
    for (const chunk of chunks) {
      result.set(chunk, offset);
      offset += chunk.length;
    }
    return result;
  }, [getStream]);

  const stopMicrophone = useCallback(async () => {
    mediaStreamRef.current?.getTracks().forEach((track) => track.stop());
    mediaStreamRef.current = null;
    recorderRef.current?.disconnect();
    const context = audioCtxRef.current;
    if (context && context.state !== "closed") await context.close();
    audioCtxRef.current = null;
  }, []);

  // One native pause covers countdowns, all ten clips, and profile processing.
  const handleEnroll = useCallback(async () => {
    if (enrollmentRef.current) return;
    const controller = new AbortController();
    enrollmentRef.current = controller;
    const { signal } = controller;
    setEnrolling(true);
    setError(null);
    setSuccess(null);
    setCurrentClip(0);

    try {
      await withEnrollmentMicrophone(signal,
        () => invoke("pause_wakeword"),
        async () => {
          const collectedClips: Float32Array[] = [];
          for (let i = 0; i < NUM_CLIPS; i++) {
            signal.throwIfAborted();
            setCurrentClip(i + 1);
            for (let c = 3; c > 0; c--) {
              setCountdown(c);
              await enrollmentDelay(1000, signal);
            }
            setCountdown(0);
            setRecording(true);
            collectedClips.push(await recordClip(signal));
            setRecording(false);
            await enrollmentDelay(500, signal);
          }
          signal.throwIfAborted();
          await invoke("enroll_voice", {
            clips: collectedClips.map((clip) => Array.from(clip)),
            threshold: null,
          });
        },
        stopMicrophone,
        () => invoke("resume_wakeword"),
      );
      if (mountedRef.current) {
        setSuccess("Profile enrolled with a threshold measured from five held-out clips. Live non-owner trials are still required.");
        await refreshStatus();
      }
    } catch (err) {
      if (mountedRef.current) setError(`Enrollment failed: ${err instanceof Error ? err.message : String(err)}`);
    } finally {
      enrollmentRef.current = null;
      if (mountedRef.current) {
        setEnrolling(false);
        setRecording(false);
        setCountdown(0);
      }
    }
  }, [recordClip, refreshStatus, stopMicrophone]);

  // Delete profile
  const handleDelete = useCallback(async () => {
    try {
      await invoke("delete_voice_profile");
      setSuccess("Profile deleted. If Voice Lock was on, voice turns remain blocked until you re-enroll or turn it off in Command Hub.");
      await refreshStatus();
    } catch (err) {
      setError(`Failed to delete: ${err instanceof Error ? err.message : String(err)}`);
    }
  }, [refreshStatus]);

  // Abort waits/recording on navigation. The session's finally releases the baton.
  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
      enrollmentRef.current?.abort(new Error("Enrollment cancelled"));
      mediaStreamRef.current?.getTracks().forEach((track) => track.stop());
    };
  }, []);

  return (
    <section className="setup-section">
      <h2>Voice Lock</h2>
      <p className="setup-hint">
        Record your voice so NEXUS can check who is speaking. The profile stays
        encrypted on this device. Keep Voice Lock off until the live owner and
        non-owner trials are complete. Record “Nexus”, “Hey Nexus”, and “OK Nexus”
        as prompted. Wake listening is paused throughout enrollment.
      </p>

      {error && <div className="setup-error">{error}</div>}
      {success && <div className="setup-saved">{success}</div>}

      {/* Status display */}
      {status && (
        <div className="voice-status">
          {status.enrolled ? (
            <div className="voice-status-enrolled">
              <span className="setup-badge setup-badge--ok">
                Profile enrolled ({status.num_clips} clips)
              </span>
              <button
                className="setup-btn setup-btn--small setup-btn--danger"
                onClick={handleDelete}
                disabled={enrolling}
              >
                Delete Profile
              </button>
            </div>
          ) : (
            <span className="setup-badge setup-badge--warn">
              No valid voice profile enrolled
            </span>
          )}
        </div>
      )}

      {/* Enrollment UI */}
      {enrolling && (
        <div className="voice-enroll-progress">
          <div className="voice-enroll-clip">
            Clip {currentClip} of {NUM_CLIPS} — {currentClip <= 5 ? "enrollment" : "held-out calibration"}
          </div>
          {countdown > 0 ? (
            <div className="voice-enroll-countdown">
              Get ready... {countdown}
            </div>
          ) : recording ? (
            <div className="voice-enroll-recording">
              🎙️ Say “{ENROLLMENT_PHRASES[currentClip - 1]}” now{currentClip >= 8 ? " from a different distance" : ""}!
            </div>
          ) : (
            <div className="voice-enroll-wait">Processing...</div>
          )}
          {/* Progress bar */}
          <div className="voice-enroll-bar">
            <div
              className="voice-enroll-bar-fill"
              style={{ width: `${(currentClip / NUM_CLIPS) * 100}%` }}
            />
          </div>
        </div>
      )}

      {/* Enroll button */}
      {!enrolling && (
        <button
          className="setup-btn"
          onClick={handleEnroll}
          disabled={enrolling}
        >
          {status?.enrolled ? "Re-enroll Voice" : "Enroll Voice"}
        </button>
      )}
    </section>
  );
}
