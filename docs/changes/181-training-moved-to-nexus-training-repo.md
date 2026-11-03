# 181 - Training folders moved to the NEXUS-Training repo

Date: 2026-10-31. The app repo root had a dozen training folders. They now live in one place: `D:\NEXUS-Training`
(private GitHub repo [Engine-NEXUS/NEXUS-Training](https://github.com/Engine-NEXUS/NEXUS-Training)).

## What moved
| Was in `D:\ULTRON` | Now | In git? |
|---|---|---|
| `wake_word_data` (238,876 files), `wake_camp`, `wake_test_recordings`, `nexus_real_samples`, `nexus_model`, `audioset`, `audioset_16k`, `mit_rirs` | `D:\NEXUS-Training\data\<name>` | **No.** Local only, ignored in both repos. |
| `train_nexus_oww_v2.ipynb`, `train_nexus_oww_v3.ipynb`, `training/train_nexus_wakeword_kaggle.ipynb` | `notebooks/` | Yes (they were never in git here: `*.ipynb` is ignored) |
| `kaggle_nexus_v2/` | `kaggle/` | Yes |

The eight data folders are still reachable at their old paths through directory junctions, so `nexus wake ...`, the trainer
scripts, the manifests and CI keep working with no code change. Git still ignores them. Two empty stray folders were removed.

## What stayed, and why
`scripts/` training and evaluation tools and `server/nlu/` (the NLU dataset and trainer) stay in the app repo: the `nexus` CLI and CI call them by relative path.
`test_doc.wav` stays (two scripts read it).

## Rules kept
Training data is never pushed or deleted; only trained models ship, and only from the app repo. The new repo's `.gitignore` blocks `/data/`, audio and model file types.
