-- Ready to merge: the change is in the inbox; a person merges it in the app or on the forge. If the default branch
-- moved on and the change no longer merges cleanly, it goes back to In progress with the conflict.
function on_card(card)
  local change = card.change
  if change ~= nil and change.conflict ~= nil then
    moebius.move("In progress", "The change no longer merges cleanly into " .. change.base .. ". These files conflict:\n"
      .. change.conflict .. "\nMerge " .. change.base .. " into your branch and resolve the conflicts.")
  else
    moebius.wait("waiting for a person to merge")
  end
end
