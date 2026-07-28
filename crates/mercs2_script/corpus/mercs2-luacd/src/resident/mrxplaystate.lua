import("MrxHqManager")
import("MrxStarterManager")
import("WifPmcInterior")
import("WifFreePlay")
import("MrxMusic")
_knNull = -1
_knFree = 0
_knMission = 1

function IsValidState(nState)
  return nState ~= _knNull or nState ~= _knFree or nState ~= _knMission
end

function GetStateDisplayName(nState)
  local sDisplayName
  if nState == _knNull then
    sDisplayName = "null"
  elseif nState == _knFree then
    sDisplayName = "free"
  elseif nState == _knMission then
    sDisplayName = "mission"
  end
  ASSERT(sDisplayName)
  return sDisplayName
end

function Set(nState)
  if IsValidState(nState) then
    if nState ~= _nCurrState then
      _nCurrState = nState
      Debug.Printf("Play state: " .. GetStateDisplayName(_nCurrState))
      if nState == _knFree then
        _oCurrMission = nil
        MrxMusic.EnterFreeplayMusic()
        WifFreePlay.StartNag()
      else
        WifFreePlay.StopNag()
      end
      Pda.Map:SetMissionChangeAllowed({
        bAllow = nState == _knFree
      })
      _UpdateHqObjectiveMarkers()
    else
      Debug.Printf("Play state set attempt FAILED; the " .. GetStateDisplayName(_nCurrState) .. " state is already set.")
      return false
    end
  else
    Debug.Printf("Play state set attempt FAILED; no valid state corresponding to value " .. _nCurrState .. ".")
    return false
  end
  return true
end

function Get()
  return _nCurrState
end

function SetCurrentMission(oMission)
  if oMission and oMission.IsContract and oMission:IsContract() then
    Debug.Printf("Current contract set to " .. oMission:GetName() .. ".")
    _oCurrMission = oMission
    return true
  else
    Debug.Printf("Current contract set attempt FAILED; input is NOT a contract.")
    return false
  end
end

function GetCurrentMission()
  return _oCurrMission
end

function IsFree()
  return Get() == _knFree
end

function Reset()
  _nCurrState = nil
  _oCurrMission = nil
  MrxMusic.EnterFreeplayMusic()
  MrxStarterManager.DestroyAllStarters()
end

function _UpdateHqObjectiveMarkers()
  local tStarters = MrxStarterManager.GetStarters()
  for sStarterName, oStarter in pairs(tStarters) do
    local sHqName = oStarter:GetHq()
    local bPmcStarter = oStarter:IsPmcStarter()
    if sHqName then
      local oHq = MrxHqManager.GetHq(sHqName)
      if oHq then
        oHq:RefreshUiDisplay()
      end
    elseif bPmcStarter then
      WifPmcInterior.RefreshUiDisplay()
    end
  end
end

function GetTotalTimeElapsed()
  local nPriorSessions = GetTimeElapsedInPriorSessions()
  local nThisSession = Sys.TimeStampGetElapsed(_uSessionStartTimestamp)
  if type(nPriorSessions) == "number" and type(nThisSession) == "number" then
    return math.ceil(nPriorSessions + nThisSession)
  else
    return Sys.MainTime()
  end
end

function StartSessionTimer()
  _uSessionStartTimestamp = Sys.MainTimeStamp()
end

function GetSessionTimer()
  return _uSessionStartTimestamp
end

function SetTimeElapsedInPriorSessions(n)
  _nTimeElapsedInPriorSessions = n
end

function GetTimeElapsedInPriorSessions()
  return _nTimeElapsedInPriorSessions
end
