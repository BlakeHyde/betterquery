- design/ is human authored and should not be modified.
- In cases where implementation contradicts design/, design/ takes precedence.
- If there is ambiguity in design/, stop and ask a question.
- There is no need to document counterfactuals in the code ("This addresses...")
  that's for the chat or commit messages.
- All source control is managed through jj. If implementing a multi-step plan
  agentically, separate each step into a new jj changeset with a short,
  descriptive message. 
