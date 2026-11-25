# Makes test speech WITHOUT a microphone: Windows' own text-to-speech renders a sentence to a 16 kHz mono 16-bit WAV and
# writes a JSON file with the time (ms from the start of the audio) at which each word begins, so a caption test can measure
# how long after a word was spoken it showed up. Nothing is played aloud and nothing leaves the machine.
#
#   powershell -File scripts/make-test-speech.ps1 -Text "open the command hub" -Out D:\nexus-test-speech\one.wav [-Voice "Microsoft Zira Desktop"] [-Rate 0]
#
# (The word times are collected inside a compiled helper: PowerShell's own event queue dropped about half of them.)
param([Parameter(Mandatory)][string]$Text, [Parameter(Mandatory)][string]$Out, [string]$Voice = "Microsoft David Desktop", [int]$Rate = 0)
Add-Type -AssemblyName System.Speech
if (-not ("NexusSpeechMaker" -as [type])) {
    Add-Type -ReferencedAssemblies System.Speech -TypeDefinition @"
using System; using System.Collections.Generic; using System.Speech.Synthesis; using System.Speech.AudioFormat;
public static class NexusSpeechMaker {
  public static List<string> Make(string text, string path, string voice, int rate) {
    var words = new List<string>();
    using (var s = new SpeechSynthesizer()) {
      s.SelectVoice(voice); s.Rate = rate;
      s.SetOutputToWaveFile(path, new SpeechAudioFormatInfo(16000, AudioBitsPerSample.Sixteen, AudioChannel.Mono));
      s.SpeakProgress += (o, e) => { lock (words) words.Add(e.Text + "\t" + (int)e.AudioPosition.TotalMilliseconds); };
      s.Speak(text);
      s.SetOutputToNull();
    }
    return words;
  }
}
"@
}
$dir = Split-Path $Out
if ($dir) { New-Item -ItemType Directory -Force -Path $dir | Out-Null }
$raw = [NexusSpeechMaker]::Make($Text, $Out, $Voice, $Rate)
$words = @($raw | ForEach-Object { $p = $_ -split "`t"; [pscustomobject]@{ word = $p[0]; startMs = [int]$p[1] } })
$json = [pscustomobject]@{ text = $Text; voice = $Voice; words = $words } | ConvertTo-Json -Depth 5
[System.IO.File]::WriteAllText([System.IO.Path]::ChangeExtension($Out, "words.json"), $json, (New-Object System.Text.UTF8Encoding($false)))
Write-Output ("wrote {0} ({1} bytes), {2} word times" -f $Out, (Get-Item $Out).Length, $words.Count)
