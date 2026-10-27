-- Backlog: issues from outside wait here until a person approves them; trusted authors' issues move on.
function on_card(card)
  if card.trusted then
    moebius.move("Ready")
  else
    moebius.ask("approve_issue", "waiting for a person to approve the issue")
  end
end
