import("MrxUtil")
import("MrxGui")
import("MrxSoundCategories")
import("MrxVoSequence")
import("MrxMunitionsPickup")
import("MrxGuiInterface")
STATE_NONE = 0
STATE_CINEMATIC = 1
STATE_WAITFORSTREAMING = 2
STATE_WAITFORTETHER = 3
STATE_WAITFORGAME = 4
_States = {
  [STATE_CINEMATIC] = {
    Enter = function()
      _StateComplete(STATE_CINEMATIC)
    end,
    Exit = function()
    end,
    nRefCount = 0,
    tEnterCompleteCallbacks = {},
    tReadyToExitCallbacks = {},
    sName = "STATE_CINEMATIC",
    safeEnterCount = 0,
    forceExitCount = 0
  },
  [STATE_WAITFORSTREAMING] = {
    Enter = function()
      Sys.RequestGameState("WaitForStreaming")
      Event.Create(Event.GameStateChange, {
        "WaitForStreaming",
        "exit"
      }, _StateComplete, {STATE_WAITFORSTREAMING})
    end,
    Exit = function()
    end,
    nRefCount = 0,
    tEnterCompleteCallbacks = {},
    tReadyToExitCallbacks = {},
    sName = "STATE_WAITFORSTREAMING",
    safeEnterCount = 0,
    forceExitCount = 0
  },
  [STATE_WAITFORTETHER] = {
    Enter = function()
      Sys.RequestGameState("WaitForTether")
      Event.Create(Event.GameStateChange, {
        "WaitForTether",
        "exit"
      }, _StateComplete, {STATE_WAITFORTETHER})
    end,
    Exit = function()
    end,
    nRefCount = 0,
    tEnterCompleteCallbacks = {},
    tReadyToExitCallbacks = {},
    sName = "STATE_WAITFORTETHER",
    safeEnterCount = 0,
    forceExitCount = 0
  },
  [STATE_WAITFORGAME] = {
    Enter = function()
      _StateComplete(STATE_WAITFORGAME)
    end,
    Exit = function()
    end,
    nRefCount = 0,
    tEnterCompleteCallbacks = {},
    tReadyToExitCallbacks = {},
    sName = "STATE_WAITFORGAME",
    safeEnterCount = 0,
    forceExitCount = 0
  }
}
_bEnableFade = true
_bUseQuickFade = false
_nQuickFadeOutTime = 0.1
_nQuickFadeInTime = 0.5
_nLongFadeOutTime = 1.1
_nLongFadeInTime = 1.1

function _GlobalEnter(fComplete, tData)
  Debug.Printf("###! GlobalEnter - Begin")
  if _bUseQuickFade then
    if _bEnableFade then
      Debug.Printf("Quick Fading-Out...")
      MrxGui.FadeToColor(_nQuickFadeOutTime)
    end
    Event.Create(Event.TimerRelative, {_nQuickFadeOutTime, true}, fComplete, tData)
  elseif _bEnableFade then
    Debug.Printf("Long Fading-Out...")
    MrxGui.GlobalFadeToBlack(fComplete, tData)
  else
    Event.Create(Event.TimerRelative, {_nLongFadeOutTime, true}, fComplete, tData)
  end
  _bQuickFaded = _bUseQuickFade
  MrxVoSequence.Stop(true)
  MrxSoundCategories.DuckMasterVolume(0.5)
  Graphics.Atmosphere.EnableImmediatelyChangeMode(true)
  MrxMunitionsPickup.ImmediatePickup()
  local tPlayers = Player.GetAllPlayers()
  if tPlayers then
    for _, uPlayer in ipairs(tPlayers) do
      local uChar = Player.GetCharacter(uPlayer)
      Player.SetScopeEnabled(uPlayer, false)
      Player.SetInputEnabled(uPlayer, false)
      if uChar then
        Object.SetInvincible(uChar, true, "MrxState")
      end
    end
  end
  Pda:SetSuppressed({vPlayer = nil, bSuppress = true})
  Hud.ResourceCounter:SetSuppressed({bSuppressCash = true, bSuppressFuel = true})
  MrxGuiInterface.HudInterface.FanfareQueue.Pause(true)
end

function _GlobalExit()
  Debug.Printf("###! GlobalExit - Begin")
  if Net.IsServer() then
    Net.SetLoadingScreen(false)
  end
  Gui.OnGlobalExit()
  local nFadeInTime
  if _bQuickFaded then
    if _bEnableFade then
      Debug.Printf("Quick Fading-In...")
      MrxGui.FadeFromColor(_nQuickFadeInTime)
    end
    nFadeInTime = _nQuickFadeInTime
  else
    if _bEnableFade then
      Debug.Printf("Long Fading-In...")
      MrxGui.GlobalFadeFromBlack()
    end
    nFadeInTime = _nLongFadeInTime
  end
  _bQuickFaded = nil
  EnableFade(true)
  MrxSoundCategories.UnduckMasterVolume(0.5)
  Graphics.Atmosphere.EnableImmediatelyChangeMode(false)
  Pda:SetSuppressed({vPlayer = nil, bSuppress = false})
  Hud.ResourceCounter:SetSuppressed({bSuppressCash = false, bSuppressFuel = false})
  Debug.Printf("Fade-In in " .. nFadeInTime .. "s...")
  Event.Create(Event.TimerRelative, {nFadeInTime, true}, function()
    local tPlayers = Player.GetAllPlayers()
    if tPlayers then
      for _, uPlayer in ipairs(tPlayers) do
        local uChar = Player.GetCharacter(uPlayer)
        Player.SetScopeEnabled(uPlayer, true)
        Player.SetInputEnabled(uPlayer, true)
        if uChar then
          Object.SetInvincible(uChar, false, "MrxState")
        end
      end
    end
    local tCallbacks = _tGlobalExitCallbacks
    _tGlobalExitCallbacks = {}
    MrxUtil.ProcessCallbackTable(tCallbacks)
    MrxGuiInterface.HudInterface.FanfareQueue.Pause(false)
    Debug.Printf("###! GlobalExit - Complete")
  end)
end

_bStateComplete = true
_fReadyToExitCallback = nil
_tReadyToExitCallbackData = nil
_tGlobalExitCallbacks = {}
_tGlobalEnterCallbacks = {}

function Reset()
  _bGloballyLocked = nil
  _bGloballyFading = nil
  _bStateComplete = true
  _fReadyToExitCallback = nil
  _tGlobalExitCallbacks = {}
  _tReadyToExitCallbackData = nil
  _tGlobalEnterCallbacks = {}
end

function _StateComplete(nState)
  local tStateData = _States[nState]
  Debug.Printf("@@@@@@@@@@ MrxState._StateComplete:  state " .. GetStateName(nState) .. ", about to _AttemptGlobalExit")
  tStateData.bLocked = false
  if nState == STATE_WAITFORTETHER then
    tStateData.nRefCount = 0
  end
  local tCallbacks = tStateData.tReadyToExitCallbacks
  tStateData.tReadyToExitCallbacks = {}
  MrxUtil.ProcessCallbackTable(tCallbacks)
  _AttemptGlobalExit()
end

function Enter(nState, fEnterCompleteCallback, tEnterCompleteCallbackData, fReadyToExitCallback, tReadyToExitCallbackData)
  local tStateData = _States[nState]
  if not tStateData then
    return false
  end
  if fEnterCompleteCallback == "nil" then
    fEnterCompleteCallback = nil
  end
  if tEnterCompleteCallbackData == "nil" then
    tEnterCompleteCallbackData = nil
  end
  if fReadyToExitCallback == "nil" then
    fReadyToExitCallback = nil
  end
  if tReadyToExitCallbackData == "nil" then
    tReadyToExitCallbackData = nil
  end
  if _bGloballyFading then
    table.insert(_tGlobalEnterCallbacks, {
      Enter,
      {
        nState,
        fEnterCompleteCallback or "nil",
        tEnterCompleteCallbackData or "nil",
        fReadyToExitCallback or "nil",
        tReadyToExitCallbackData or "nil"
      }
    })
    return false
  end
  tStateData.nRefCount = tStateData.nRefCount + 1
  Debug.Printf("@@@@@@@@@@ MrxState.Enter: state " .. GetStateName(nState) .. " (refcount=" .. tostring(tStateData.nRefCount) .. ")")
  Debug.Printf(Debug.GetCallstack())
  table.insert(tStateData.tEnterCompleteCallbacks, {fEnterCompleteCallback, tEnterCompleteCallbackData})
  table.insert(tStateData.tReadyToExitCallbacks, {fReadyToExitCallback, tReadyToExitCallbackData})
  if not _bGloballyLocked and not _bGloballyFading then
    _bGloballyLocked = true
    _bGloballyFading = true
    _GlobalEnter(_CompleteEnter, {tStateData})
    return true
  end
  if not _bGloballyFading then
    _CompleteEnter(tStateData)
  end
  return true
end

function _CompleteEnter(tStateData)
  _bGloballyFading = nil
  local tCallbacks = tStateData.tEnterCompleteCallbacks
  tStateData.tEnterCompleteCallbacks = {}
  MrxUtil.ProcessCallbackTable(tCallbacks)
  if not tStateData.bLocked then
    tStateData.bLocked = true
    tStateData.Enter()
  end
  local tCallbacks = _tGlobalEnterCallbacks
  _tGlobalEnterCallbacks = {}
  MrxUtil.ProcessCallbackTable(tCallbacks)
  Debug.Printf("###! GlobalEnter - Complete")
end

function Exit(nState, fCallback, tCallbackData)
  local tStateData = _States[nState]
  if not tStateData then
    return false
  end
  if tStateData.nRefCount <= 0 then
    Debug.Printf("@@@@@@@@@@ MrxState.Exit: UNPAIRED EXIT to state " .. GetStateName(nState))
    Debug.Printf(Debug.GetCallstack())
    MrxUtil.CallWithOptionalArgs(fCallback, tCallbackData)
    return false
  end
  if _bGloballyFading then
    table.insert(_tGlobalEnterCallbacks, {
      Exit,
      {
        nState,
        fCallback,
        tCallbackData
      }
    })
    return
  end
  table.insert(_tGlobalExitCallbacks, {fCallback, tCallbackData})
  tStateData.nRefCount = tStateData.nRefCount - 1
  Debug.Printf("@@@@@@@@@@ MrxState.Exit: state " .. GetStateName(nState) .. " (refcount=" .. tostring(tStateData.nRefCount) .. ")")
  Debug.Printf(Debug.GetCallstack())
  if tStateData.nRefCount == 0 and tStateData.Exit then
    tStateData.Exit()
  end
  _AttemptGlobalExit()
  return true
end

function _AttemptGlobalExit()
  Debug.Printf("@@@@@@@@@@ MrxState._AttemptGlobalExit")
  if not _bGloballyLocked then
    Debug.Printf("@@@@@@@@@@ MrxState._AttemptGlobalExit: not globally locked, bailing out")
    return
  end
  local bAllStatesExited = true
  for nState, tStateData in ipairs(_States) do
    if tStateData.nRefCount > 0 or tStateData.bLocked then
      Debug.Printf("@@@@@@@@@@ MrxState._AttemptGlobalExit:  state " .. GetStateName(nState) .. " still active .. (refcount=" .. tostring(tStateData.nRefCount) .. ",bLocked=" .. tostring(tStateData.bLocked) .. ")")
      bAllStatesExited = false
      break
    end
  end
  if bAllStatesExited then
    Debug.Printf("@@@@@@@@@@ MrxState._AttemptGlobalExit: all states exited; success")
    _GlobalExit()
    _bGloballyLocked = nil
  end
end

function _GetTotalRefCount()
  local retVal = 0
  for nState, tStateData in ipairs(_States) do
    retVal = retVal + tStateData.nRefCount
  end
  return retVal
end

function IsLocked()
  return _bGloballyLocked
end

function SetQuickFade(bEnable)
  _bUseQuickFade = bEnable
end

function EnableFade(bEnable)
  Debug.Printf("@@@@@@@@@@ MrxState.EnableFade: bEnable=" .. tostring(bEnable))
  _bEnableFade = bEnable
end

function PrintStatus()
  local bAnyActiveStates = false
  for nState, tStateData in ipairs(_States) do
    if tStateData.nRefCount > 0 or tStateData.bLocked then
      Debug.Printf("@@@@@@@@@@ MrxState.PrintStatus:  state " .. GetStateName(nState) .. " still active .. (refcount=" .. tostring(tStateData.nRefCount) .. ",bLocked=" .. tostring(tStateData.bLocked) .. ")")
      bAnyActiveStates = true
    end
  end
  if not bAnyActiveStates then
    Debug.Printf("@@@@@@@@@@ MrxState.PrintStatus: no active states")
  end
end

function GetStateName(nState)
  if _States[nState] then
    return _States[nState].sName
  end
end

function SafeEnterCallback(nState)
  local tStateData = _States[nState]
  if not tStateData then
    return false
  end
  tStateData.safeEnterCount = tStateData.safeEnterCount + 1
  if tStateData.forceExitCount <= tStateData.safeEnterCount then
    for i = 1, tStateData.forceExitCount do
      tStateData.safeEnterCount = tStateData.safeEnterCount - 1
      Exit(nState)
    end
    tStateData.forceExitCount = 0
  end
end

function SafeEnter(nState)
  local tStateData = _States[nState]
  if not tStateData then
    return false
  end
  Enter(nState, SafeEnterCallback, {nState})
end

function SafeExit(nState)
  local tStateData = _States[nState]
  if not tStateData then
    return false
  end
  if tStateData.safeEnterCount > 0 then
    tStateData.safeEnterCount = tStateData.safeEnterCount - 1
    Exit(nState)
  else
    tStateData.forceExitCount = tStateData.forceExitCount + 1
  end
end

function AddGlobalExitCallback(fCallback, tCallbackArgs)
  if _bGloballyLocked then
    table.insert(_tGlobalExitCallbacks, {fCallback, tCallbackArgs})
  else
    MrxUtil.CallWithOptionalArgs(fCallback, tCallbackArgs)
  end
end
