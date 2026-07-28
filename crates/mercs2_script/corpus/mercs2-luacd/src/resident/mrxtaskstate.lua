_knLatent = 0
_knActive = 1
_knCompleted = 2
_knCancelled = 3

function IsValidState(nState)
  return nState == _knLatent or nState == _knActive or nState == _knCompleted or nState == _knCancelled
end

function GetStateDisplayName(nState)
  local sDisplayName
  if nState == _knLatent then
    sDisplayName = "latent"
  elseif nState == _knActive then
    sDisplayName = "active"
  elseif nState == _knCompleted then
    sDisplayName = "completed"
  elseif nState == _knCancelled then
    sDisplayName = "cancelled"
  end
  ASSERT(sDisplayName)
  return sDisplayName
end
