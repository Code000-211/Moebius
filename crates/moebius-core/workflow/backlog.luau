-- Backlog: issues from outside wait here until a person approves them; trusted authors' issues move on.
function on_card(card)
  if card.trusted then
    moebius.move("Ready")
    return
  end
  local choice = moebius.decide(
    "approve_issue",
    { title = card.title, body = card.body, author = card.author },
    { "approve", "wait" },
    "wait"
  )
  if choice == "approve" then
    moebius.move("Ready")
  else
    moebius.wait("waiting for a person to approve the issue")
  end
end
