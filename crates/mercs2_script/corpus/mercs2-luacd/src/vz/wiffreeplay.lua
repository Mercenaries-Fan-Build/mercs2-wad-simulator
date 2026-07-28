import("MrxActionHijack")
import("MrxHqManager")
import("MrxPlayState")
import("MrxUtil")
import("MrxVoSequence")
import("WifHints")
import("WifPmcInterior")
local _knInitialDelay = 60
local _knSubsequentDelay = 600
local _knRetryDelay = 30

function StartNag()
  if _bNagEnabled then
    return
  end
  _bNagEnabled = true
  _CreateNagTimer(_knInitialDelay)
end

function StopNag()
  if not _bNagEnabled then
    return
  end
  if _bNagInProgress then
    MrxVoSequence.Stop(nil, nil, MrxVoSequence.knPriorityFreeplay)
  end
  _DeleteNagTimer()
  _bNagEnabled = nil
end

function IsNagEnabled()
  return _bNagEnabled == true
end

function _CreateNagTimer(nTime)
  _DeleteNagTimer()
  if _TestNagConditions() then
    _uNagTimer = Event.Create(Event.TimerRelative, {nTime}, _Nag)
  else
    StopNag()
  end
end

function _DeleteNagTimer()
  if _uNagTimer then
    Event.Delete(_uNagTimer)
    _uNagTimer = nil
  end
end

function _Nag()
  if MrxVoSequence.IsSequenceInProgress() then
    _CreateNagTimer(_knRetryDelay)
    return
  end
  if not _TestNagConditions() then
    StopNag()
    return
  end
  local tCues = {
    "Fiona.Misc.NoState01",
    "Fiona.Misc.NoState02"
  }
  local sCue = MrxUtil.GetRandomTableElement(tCues)
  local tSequence = {sCue, _NagComplete}
  local bSuccess = MrxVoSequence.Start(tSequence, nil, MrxVoSequence.knPriorityFreeplay)
  if bSuccess then
    _bNagInProgress = true
  else
    _CreateNagTimer(_knRetryDelay)
  end
end

function _NagComplete()
  _bNagInProgress = nil
  _CreateNagTimer(_knSubsequentDelay)
end

function _TestNagConditions()
  return not _bNagInProgress and MrxPlayState.IsFree() and not MrxActionHijack.IsInHijack() and not MrxHqManager.IsInside() and not WifPmcInterior.IsInside() and WifHints.HasHint("Fiona")
end
