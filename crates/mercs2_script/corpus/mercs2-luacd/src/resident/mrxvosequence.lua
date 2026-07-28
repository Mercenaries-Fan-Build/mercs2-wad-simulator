import("MrxUtil")
import("MrxSoundCategories")
knPriorityCinematic = VO.PRIORITY_CINEMATIC
knPriorityBriefing = VO.PRIORITY_SCRIPTED_BRIEFING
knPriorityContract = VO.PRIORITY_SCRIPTED_CONTRACT
knPriorityBounties = VO.PRIORITY_SCRIPTED_BOUNTIES
knPriorityFreeplay = VO.PRIORITY_SCRIPTED_FREEPLAY

function Start(vSequence, bCinematic, nPriority, bSendNetEvent)
  bSendNetEvent = MrxUtil.SetDefault(bSendNetEvent, true)
  local sType = type(vSequence)
  if sType == "string" then
    vSequence = {vSequence}
  end
  if not sType == "table" then
    Debug.Printf("@@@@@@@@@@ MrxVoSequence.Start: sequence format is unusable!  (type: " .. sType .. ")")
    return false
  end
  if type(bCinematic) ~= "boolean" then
    bCinematic = false
  end
  if type(nPriority) ~= "number" then
    nPriority = knPriorityContract
  end
  if bCinematic then
    nPriority = knPriorityCinematic
  end
  local tSpeakers = {}
  local tFormattedSequence = {}
  for i, vStage in ipairs(vSequence) do
    local sStageType = type(vStage)
    if sStageType == "string" or sStageType == "number" or sStageType == "function" then
      vStage = {vStage}
    end
    local tStage
    if vStage[1] then
      local vArg1 = vStage[1]
      local vArg2 = vStage[2]
      local sArg1Type = type(vArg1)
      local sArg2Type = type(vArg2)
      if sArg1Type == "string" then
        local vSpeaker = vArg2
        if sArg2Type == "string" then
          vSpeaker = Pg.GetGuidByName(vSpeaker)
        elseif sArg2Type == "nil" then
          vSpeaker = 0
        end
        tStage = {vSpeaker = vSpeaker, sCue = vArg1}
        table.insert(tSpeakers, vSpeaker)
      elseif sArg1Type == "function" then
        local vArg3 = vStage[3]
        tStage = {
          fCallback = vArg1,
          tCallbackArgs = vArg2,
          bIgnoreOnSkip = vArg3
        }
      elseif sArg1Type == "number" then
        if vArg1 < 0 then
          vArg1 = 0
        end
        tStage = {nDelay = vArg1}
      end
    else
      local id = MrxUtil.GetCharacterIdentity(Player.GetPrimaryCharacter())
      if id then
        tStage = {
          sCue = vStage[id],
          vSpeaker = Player.GetPrimaryCharacter()
        }
      else
        Debug.Printf("ERROR: primary player character is not J / M / C")
      end
    end
    if tStage then
      table.insert(tFormattedSequence, tStage)
    end
  end
  local bFadeSound = true
  if _tSequence then
    if nPriority <= _tSequence.nPriority then
      bFadeSound = false
      Stop(bFadeSound)
    else
      Debug.Printf("VO sequence start attempt FAILED; a VO sequence is already in progress")
      _CallSequenceCallbacks(tFormattedSequence)
      return false
    end
  end
  _tSequence = tFormattedSequence
  _tSequence.nPriority = nPriority
  _tSequence.bSendNetEvent = bSendNetEvent
  _nBaseDelay = 0.25
  if type(vSequence.nBaseDelay) == "number" and 0 <= vSequence.nBaseDelay then
    _nBaseDelay = vSequence.nBaseDelay
  end
  if bFadeSound then
    MrxSoundCategories.Fade("vosequence", true)
  end
  table.insert(tSpeakers, Player.GetPrimaryCharacter())
  local uSecondaryCharacter = Player.GetSecondaryCharacter()
  if uSecondaryCharacter then
    table.insert(tSpeakers, uSecondaryCharacter)
  end
  for i, uSpeaker in ipairs(tSpeakers) do
    VO.AddSequence(uSpeaker, nPriority)
  end
  _tSequence.tSpeakers = tSpeakers
  _ExecuteStage(1)
  return true
end

function _ExecuteStage(nStage)
  if not _tSequence then
    Debug.Printf("MrxVoSequence: _ExecuteStage called, but no sequence exists.  Probably from delayed call (delay happened after Stop was called)  Bailing out...")
    return
  end
  local tStage = _tSequence[nStage]
  if not tStage then
    Cleanup()
    return
  end
  
  local function _NextStage(sVoState)
    Debug.Printf("MrxVoSequence._NextStage: sVoState=" .. tostring(sVoState))
    if _bStoppingSequence then
      return
    end
    if not _tSequence then
      Debug.Printf("MrxVoSequence: _NextStage called, but no sequence exists.  Likely, the sequence was stopped from one of its own callbacks.  Bailing out...")
      return
    end
    if _uTimeoutEvent then
      Event.Delete(_uTimeoutEvent)
      _uTimeoutEvent = nil
    end
    local nNextStage = nStage + 1
    local tNextStage = _tSequence[nNextStage]
    Debug.Printf("MrxVoSequence: nNextStage=" .. nNextStage)
    if not tNextStage then
      Debug.Printf("MrxVoSequence: End of sequence")
      Cleanup()
      return
    end
    local nDelay = 0
    if tNextStage.nDelay then
      while tNextStage and tNextStage.nDelay do
        nDelay = nDelay + tNextStage.nDelay
        nNextStage = nNextStage + 1
        tNextStage = _tSequence[nNextStage]
      end
    elseif tNextStage.fCallback then
      nDelay = 0
    else
      nDelay = _nBaseDelay
    end
    if sVoState == "cancel" then
      nDelay = nDelay + 5
    end
    if 0 < nDelay then
      Debug.Printf("MrxVoSequence: Delaying " .. nDelay .. "s ")
      _uDelayTimer = Event.Create(Event.TimerRelative, {nDelay}, _ExecuteStage, {nNextStage})
    else
      Debug.Printf("MrxVoSequence: No delay specified...executing next stage immediately")
      _ExecuteStage(nNextStage)
    end
  end
  
  if tStage.sCue then
    Debug.Printf("MrxVoSequence: Stage " .. nStage .. ": Playing cue " .. tStage.sCue)
    local bSuccess = VO.Cue(tStage.vSpeaker, tStage.sCue, _NextStage, {}, _tSequence.nPriority, _tSequence.bSendNetEvent)
    if not bSuccess then
      Debug.Printf("MrxVoSequence: Playback of cue " .. tStage.sCue .. " FAILED!")
    end
    Pda.Database:AddLogEntry({
      sType = "dialog",
      sName = "",
      sMessage = "[" .. tStage.sCue .. "]",
      sColor = "FFFFFF"
    })
    if _knTimeout then
      _uTimeoutEvent = Event.Create(Event.TimerRelative, {_knTimeout}, VO.Cancel, {
        tStage.vSpeaker,
        tStage.sCue,
        _tSequence.bSendNetEvent
      })
      Debug.Printf("MrxVoSequence: Created timeout event " .. tostring(_uTimeoutEvent))
    end
  elseif tStage.fCallback then
    Debug.Printf("MrxVoSequence: Stage " .. nStage .. ": Calling callback...")
    tStage.bCalled = true
    MrxUtil.CallWithOptionalArgs(tStage.fCallback, tStage.tCallbackArgs)
    _NextStage()
  end
end

function Stop(bFadeSound, bIssueDanglingCallbacks, nPriorityFilter)
  if _tSequence then
    if nPriorityFilter and _tSequence.nPriority ~= nPriorityFilter then
      return
    end
    _bStoppingSequence = true
    for i, tStage in ipairs(_tSequence) do
      if tStage.sCue then
        Debug.Printf("MrxVoSequence: Stopping cue " .. tStage.sCue .. " (speaker " .. tostring(tStage.vSpeaker) .. ")")
        VO.Cancel(tStage.vSpeaker, tStage.sCue, _tSequence.bSendNetEvent)
      end
    end
    bIssueDanglingCallbacks = MrxUtil.SetDefault(bIssueDanglingCallbacks, true)
    if bIssueDanglingCallbacks then
      _CallSequenceCallbacks(_tSequence)
    end
    _bStoppingSequence = nil
    Cleanup(bFadeSound)
  end
  if _uDelayTimer then
    Event.Delete(_uDelayTimer)
    _uDelayTimer = nil
  end
  if _uTimeoutEvent then
    Event.Delete(_uTimeoutEvent)
    _uTimeoutEvent = nil
  end
end

function Cleanup(bFadeSound)
  if not _tSequence then
    return
  end
  bFadeSound = MrxUtil.SetDefault(bFadeSound, true)
  if bFadeSound then
    MrxSoundCategories.Fade("vosequence", false)
  end
  if _tSequence.tSpeakers then
    for i, uSpeaker in ipairs(_tSequence.tSpeakers) do
      VO.RemoveSequence(uSpeaker, _tSequence.nPriority)
    end
  end
  _tSequence = nil
end

function Reset()
  if _tSequence then
    MrxSoundCategories.Fade("vosequence", false)
  end
  _tSequence = nil
  _nBaseDelay = nil
  _bCinematic = nil
  _bStoppingSequence = nil
  _uTimeoutEvent = nil
  _knTimeout = nil
end

function _CallSequenceCallbacks(tFormattedSequence)
  for nStage, tStage in ipairs(tFormattedSequence) do
    if tStage.fCallback and not tStage.bIgnoreOnSkip and not tStage.bCalled then
      Debug.Printf("MrxVoSequence: Stage " .. nStage .. ": Calling callback...")
      tStage.bCalled = true
      MrxUtil.CallWithOptionalArgs(tStage.fCallback, tStage.tCallbackArgs)
    end
  end
end

function IsSequenceInProgress()
  return _tSequence ~= nil
end
