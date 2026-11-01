import json, sys
# (expected_label, [phrasings]) -- phrasings a person actually says: canonical, casual, polite, partial, STT mishearings
S = {
 "analyse_repo": ["analyse this repo", "analyze the repository", "can you analyze zync for me", "give me an analysis of the servx repo", "review my repo zync", "tell me about this repository",
                  "what does this repo do", "break down the repo for me", "analyzing repository from my github for me", "do an analysis of nexus windows repo", "i want you to analyse zync", "take a look at this repo and analyse it"],
 "analyse_pr": ["analyse pr 254 in zync", "analyze pull request 12 in servx", "look at pr number 34", "review pull request 7 of zync", "what is in pr 254", "check pr 254 for me",
                "analyse pr 254 in zink", "analyse pull request two fifty four in sir vex"],
 "analyse_latest_pr": ["analyse the latest pr", "analyze the newest pull request in zync", "review the last pr", "look at the most recent pull request", "check the latest pr in servx"],
 "list_prs": ["list pull requests", "show me open prs", "what prs are open in zync", "list all the pull requests and all", "show pull requests in servx", "any open pull requests", "how many prs are open", "show me the prs"],
 "merge_pr": ["merge pr 254 in zync", "merge pull request 12", "merge pr statistics in nexus engines", "go ahead and merge pr 254", "please merge the pr 34 in servx", "merge it", "ship pr 254", "merge number 254"],
 "approve_pr": ["approve pr 254", "approve pull request 12 in zync", "approve the pr 34", "give pr 254 an approval", "lgtm pr 254"],
 "close_pr": ["close pr 254", "close pull request 12 in zync", "close the pr 34", "shut pr 254", "reject pr 254"],
 "comment_pr": ["comment on pr 254 saying looks good", "add a comment to pull request 12 nice work", "write on pr 34 please fix the tests"],
 "create_pr": ["create a pull request from dev to main in zync", "open a pr from feature login to main", "make a pull request for my branch"],
 "check_branch": ["check branch main in zync", "show branch status", "what is the status of branch dev"],
 "list_branches": ["list branches in zync", "show me all branches", "what branches does servx have"],
 "delete_branch": ["delete branch old-login in zync", "remove the branch feature x"],
 "list_workflow_runs": ["show workflow runs in zync", "did the build pass", "is the ci green", "show me the latest actions run", "why did the build fail"],
 "rerun_workflow": ["rerun the workflow in zync", "run the failed build again", "retry the ci"],
 "list_releases": ["list releases in zync", "show me the releases", "what is the latest release of servx"],
 "create_release": ["create a release v1.2 in zync", "publish release version two"],
 "open_github_dashboard": ["open github dashboard", "show my github", "open my github feed", "take me to github"],
 "list_collaborators": ["list collaborators in zync", "who has access to servx"],
 "add_collaborator": ["add john as a collaborator to zync", "give priya access to servx"],
 "get_pr": ["show me pr 254", "open pull request 12", "details of pr 34 in zync", "get pr 254"],
 "find_pr": ["find pr 254", "which repo has pull request 254", "where is pr 254"],
 "list_pr_files": ["list files in pr 254", "which files changed in pr 12", "what did pr 254 change"],
 "revert_pr": ["revert pr 254", "undo pr 254 in zync"],
 "coderabbit_review_this": ["coderabbit review this", "code rabbit review pr 254"],
 "open_app": ["open chrome", "launch spotify", "start notepad", "can you open brave for me", "open up vs code", "open the calculator"],
 "close_app": ["close notepad", "quit chrome", "kill spotify", "shut down notepad"],
 "open_settings": ["open command hub", "nexus command hub", "command hop", "i again have a command hub", "open the settings", "show me the control panel of nexus"],
 "power_action": ["shut down my laptop", "shed down my laptop", "turn off the computer", "restart my pc", "put the laptop to sleep"],
 "enter_ghost_control": ["ghost mode", "enter ghost mode", "the post mode", "cancel the ghost mode"],
 "screen_analysis": ["what am i seeing", "analyze my screen", "what is on my screen", "look at this and tell me what it is"],
 "play_music": ["play shape of you", "play some music on spotify", "put on a song by arijit singh"],
 "search": ["search for almonds", "it's for almonds", "google best laptops", "look up weather in hyderabad"],
 "whatsapp_chat": ["open my chat with mummy", "message mom on whatsapp", "open whatsapp chat with raj"],
 "send_whatsapp_message": ["send mom a whatsapp message saying i will be late", "tell raj on whatsapp that i am coming"],
 "directions": ["directions to charminar", "how do i get to golconda fort", "navigate to secunderabad station"],
 "place_info": ["where is charminar", "tell me about paradise biryani secunderabad"],
}
rows = []
i = 0
for lab, ps in S.items():
    for p in ps:
        rows.append({"id": i, "text": p, "expect": lab}); i += 1
out = sys.argv[1]
with open(out, "w", encoding="utf8") as f:
    for r in rows:
        f.write(json.dumps(r) + "\n")
print(len(rows))
