import { SwitchCard } from "../theme/OptionCard";

interface Props {
  settings: { reminderCountdown?: boolean };
  update: (key: "reminderCountdown", value: boolean) => void;
}

/**
 * The reminder countdown option. On: a small chip stays beside the orb for every waiting reminder (trips, calendar,
 * timers, study slots) until it fires. Off: nothing stays on screen, but each reminder still fires on time and NEXUS
 * still says it. The user's choice always wins over what a reminder asks for (see src/stage/reminderModel.ts).
 */
export function ReminderSection({ settings, update }: Props) {
  const on = settings.reminderCountdown ?? true;
  return (
    <div className="settings-section" style={{ border: "none", padding: 0 }}>
      <SwitchCard
        icon="⏱"
        title="Keep the reminder countdown on screen"
        desc="A small chip beside the orb counts down to each reminder. Off: reminders still fire on time and NEXUS still says them, but nothing stays on screen."
        checked={on}
        onChange={(next) => update("reminderCountdown", next)}
      />
    </div>
  );
}
