inherit("MrxTask")
import("MrxUtil")
import("MrxGuiInterface")
import("MrxVoSequence")
local _knMarkerYClampDistance = 32

function Activated(self)
  self._tEvents = {}
  self._tTargets = {}
  self._nCompleted = 0
  self._nCancelled = 0
  self._nQuota = 0
  self._nTotal = 0
  local tConfig = self:GetConfig()
  local bTrackOnActivate = MrxUtil.SetDefault(tConfig.bTrackOnActivate, true)
  self._bDspBlpSticky = bTrackOnActivate
  self:_SetDisplaySettingsFromConfig()
  if tConfig.uTgtObjFilter then
    self._uTgtObjFilter = tConfig.uTgtObjFilter
  else
    self._uTgtObjFilter = ObjectFilter.Create()
    ObjectFilter.UsePlayers(self._uTgtObjFilter, tConfig.bTgtPlayers)
    if type(tConfig.sTgtLabelFilter) == "string" then
      ObjectFilter.SetFilter(self._uTgtObjFilter, tConfig.sTgtLabelFilter)
    end
    
    local function _ProcessElement(vElement, bExclude)
      local uGuid
      local sElementType = type(vElement)
      if sElementType == "string" then
        uGuid = Pg.GetGuidByName(vElement)
      elseif sElementType == "userdata" then
        uGuid = vElement
      end
      if uGuid and self._IsValidTarget(uGuid) then
        ObjectFilter.AddObject(self._uTgtObjFilter, uGuid, bExclude)
      end
    end
    
    local function _ProcessTargets(vTargets, bExclude)
      local sTargetsType = type(vTargets)
      if sTargetsType == "table" then
        for _, vElement in ipairs(vTargets) do
          _ProcessElement(vElement, bExclude)
        end
      else
        _ProcessElement(vTargets, bExclude)
      end
    end
    
    _ProcessTargets(tConfig.vTgtInclude, false)
    _ProcessTargets(tConfig.vTgtExclude, true)
  end
  if tConfig.nPartsCompleted then
    self._nCompleted = tConfig.nPartsCompleted
  end
  if self._uTgtObjFilter then
    self:_SetupTargets()
  end
  self:_UpdateMissionInPda()
  self:_SetDisplaySettingsFromConfig()
  self:_RefreshAllTargetDisplay()
  
  local function _AddMessageDisplayed()
    if self:IsActive() then
      self:PulsateRadarBlips(5)
      self:_CreateEvent(Event.TimerRelative, {5.5}, self._InitialNotesComplete, {self})
    else
      Debug.Printf("@@@@@@@@@@ MrxTaskObjective.Activated._AddMessageDisplayed: objective " .. self:GetName() .. " is inactive")
    end
  end
  
  local function _DisplayAddMessage()
    self._bVoSeqOnAddCompleted = true
    if tConfig.bSkipInitialNotifications then
      return
    end
    if tConfig.bDspBounty then
    elseif tConfig.bOptional then
      self:_PrintObjectiveMessage("bonus_add", _AddMessageDisplayed)
    else
      self:_PrintObjectiveMessage("add", _AddMessageDisplayed)
    end
  end
  
  local vVoSeq = tConfig.vVoSeqOnAdd
  if vVoSeq then
    local t
    if type(vVoSeq) == "table" then
      t = MrxUtil.CopyTable(vVoSeq)
    else
      t = {vVoSeq}
    end
    table.insert(t, _DisplayAddMessage)
    MrxVoSequence.Start(t)
  else
    _DisplayAddMessage()
  end
  MrxTask.Activated(self)
  if tConfig.vTgtInclude then
    MrxGui.SetObjectiveInformationCallback(DisplayTextInSatelliteMode, self)
  end
end

function _InitialNotesComplete(self)
  local tConfig = self:GetConfig()
  MrxUtil.ProcessCallbackTable(tConfig.tOnInitialNotesComplete)
  MrxUtil.CallWithOptionalArgs(tConfig.fOnInitialNotesComplete)
end

function Complete(self)
  if self:IsCompleted() then
    Debug.Printf("Completion of task " .. self:GetName() .. " FAILED; task is already completed.")
    return
  end
  Debug.Printf("Task \"" .. self:GetLineage() .. "\" complete")
  local oMission = self:GetMissionAncestor()
  self:Cleanup()
  self:_SetState(MrxTaskState._knCompleted)
  oMission:RefreshPdaDisplay()
  self:_IssueStateChangeCallbacks()
end

function Cancel(self)
  if self:IsCancelled() then
    Debug.Printf("Cancellation of task " .. self:GetName() .. " FAILED; task is already cancelled.")
    return
  end
  Debug.Printf("Task \"" .. self:GetLineage() .. "\" cancelled")
  local oMission = self:GetMissionAncestor()
  self:Cleanup()
  self:_SetState(MrxTaskState._knCancelled)
  oMission:RefreshPdaDisplay()
  self:_IssueStateChangeCallbacks()
end

function Cleanup(self)
  local tConfig = self:GetConfig()
  if tConfig.vVoSeqOnAdd and not self._bVoSeqOnAddCompleted then
    MrxVoSequence.Stop()
  end
  self._uTgtObjFilter = nil
  _ClearEventTable(self._tEvents)
  if self._uTargetOverflow then
    Event.Delete(self._uTargetOverflow)
  end
  self._uTargetOverflow = nil
  self:_SetAllTargetStatus(false)
  MrxGui.RemoveObjectiveInformation(self)
  MrxTask.Cleanup(self)
end

function _ClearEventTable(tEvents)
  for _, uEvent in pairs(tEvents) do
    if type(uEvent) == "table" then
      _ClearEventTable(uEvent)
    else
      Event.Delete(uEvent)
    end
  end
end

function CompletePart(self, ...)
  local tConfig = self:GetConfig()
  self._nCompleted = self._nCompleted + 1
  MrxUtil.ProcessCallbackTable(tConfig.tOnPartComplete, arg)
  MrxUtil.CallWithOptionalArgs(tConfig.fOnPartComplete, arg)
  if self._oTimer and tConfig.nAddTime then
    self._oTimer:AddTime(tConfig.nAddTime)
  end
  local bAllComplete = false
  local sMsgType = "upd"
  if self:IsQuotaMet() then
    bAllComplete = true
    sMsgType = "cpl"
  end
  if tConfig.bDspBounty then
    if sMsgType == "upd" then
      sMsgType = "bty_upd"
    elseif sMsgType == "cpl" then
      sMsgType = "bty_cpl"
    end
  elseif tConfig.bOptional then
    if tConfig.bDspCollectible then
      if sMsgType == "upd" then
        sMsgType = "collectible_upd"
      end
    else
      sMsgType = "bonus_upd"
    end
  end
  self:_PrintObjectiveMessage(sMsgType)
  self:_UpdateMissionInPda()
  if bAllComplete then
    self:Complete()
  end
end

function CancelPart(self, ...)
  local tConfig = self:GetConfig()
  self._nCancelled = self._nCancelled + 1
  MrxUtil.ProcessCallbackTable(tConfig.tOnPartCancel, arg)
  MrxUtil.CallWithOptionalArgs(tConfig.fOnPartCancel, arg)
  self:_UpdateMissionInPda()
  if self._nQuota > self._nTotal - self._nCancelled then
    local sMsgType = "ccl"
    if tConfig.bDspBounty then
      sMsgType = "bty_ccl"
    elseif tConfig.bOptional then
      sMsgType = "bonus_ccl"
    end
    self:_PrintObjectiveMessage(sMsgType)
    self:Cancel()
  end
end

function IsQuotaMet(self)
  return self._nCompleted == self._nQuota
end

function IsLiveConfigureable(self, sConfigKey)
  local tValidKeys = {
    nQuota = true,
    bDsp = true,
    bDspDescPda = true,
    bDspBlp = true,
    bDspBlpRdr = true,
    bDspBlpPda = true,
    bDspBlpWld = true,
    bDspMsg = true,
    bDspMsgAdd = true,
    bDspMsgUpd = true,
    bDspMsgCpl = true,
    bDspMsgCcl = true,
    tOnPartComplete = true,
    fOnPartComplete = true,
    tOnPartCancel = true,
    fOnPartCancel = true
  }
  local bFound = tValidKeys[sConfigKey]
  bFound = bFound or MrxTask.IsLiveConfigureable(self, sConfigKey)
  return bFound
end

function ReinterpretConfig(self)
  local tConfig = self:GetConfig()
  self:_SetDisplaySettingsFromConfig()
  local tIncludedObjects = ObjectFilter.GetObjects(self._uTgtObjFilter, false)
  local uAnyPlayer = Player.GetAnyCharacter()
  local uAllPlayers = Player.GetAllCharacters()
  for _, uGuid in ipairs(tIncludedObjects) do
    if uGuid ~= uAnyPlayer and uGuid ~= uAllPlayers then
      self:_SetTargetStatus(uGuid, true)
    end
  end
  self:_RefreshAllTargetDisplay()
end

function _SetupTargets(self)
  local tConfig = self:GetConfig()
  local tIncludedObjects = ObjectFilter.GetObjects(self._uTgtObjFilter, false)
  local coopGuid = ObjectFilter.GetCoopPlayerGuid(self._uTgtObjFilter)
  if coopGuid then
    self._nTotal = 1
  else
    local uAnyPlayer = Player.GetAnyCharacter()
    local uAllPlayers = Player.GetAllCharacters()
    self._nTotal = 0
    for _, uGuid in ipairs(tIncludedObjects) do
      if uGuid ~= uAnyPlayer and uGuid ~= uAllPlayers then
        self._nTotal = self._nTotal + 1
        self:_SetTargetStatus(uGuid, true)
      end
    end
  end
  self._nQuota = self._nTotal
  if type(tConfig.nQuota) == "number" then
    self._nQuota = tConfig.nQuota
  end
end

function RemoveTarget(self, uGuid)
  ObjectFilter.AddObject(self._uTgtObjFilter, uGuid, true)
  self:_SetTargetStatus(uGuid, false)
end

function _SetTargetStatus(self, uGuid, bOn, sType)
  local tGuids = self._tTargets
  if not tGuids[uGuid] then
    tGuids[uGuid] = {}
  end
  tGuids[uGuid].bStatus = bOn
  if sType then
    tGuids[uGuid].sType = sType
  end
  self:_RefreshTargetDisplay(uGuid)
end

function _SetAllTargetStatus(self, bOn)
  for uGuid, tTgtDspInfo in pairs(self._tTargets) do
    self:_SetTargetStatus(uGuid, bOn)
  end
end

function GetTargetObjectFilter(self)
  return self._uTgtObjFilter
end

function _SetDisplaySettingsFromConfig(self)
  local tConfig = self:GetConfig()
  local bDsp = MrxUtil.SetDefault(tConfig.bDsp, true)
  if bDsp then
    local bDspBlp = MrxUtil.SetDefault(tConfig.bDspBlp, true)
    local bDspMsg = MrxUtil.SetDefault(tConfig.bDspMsg, true)
    local bDspDescPda = MrxUtil.SetDefault(tConfig.bDspDescPda, true)
    self:_ToggleBlipDisplay(bDspBlp)
    self:_ToggleMsgDisplay(bDspMsg)
    self:_SetDescPdaDisplay(bDspDescPda)
    if bDspBlp then
      self:_SetBlipDisplay(tConfig)
    end
    if bDspMsg then
      self:_SetMsgDisplay(tConfig)
    end
  else
    self:_ToggleBlipDisplay(bDsp)
    self:_ToggleMsgDisplay(bDsp)
    self:_SetDescPdaDisplay(bDsp)
  end
end

function _ToggleBlipDisplay(self, bOn)
  local tConfig = {
    bDspBlpRdr = bOn,
    bDspBlpPda = bOn,
    bDspBlpWld = bOn
  }
  self:_SetBlipDisplay(tConfig)
end

function _SetBlipDisplay(self, tConfig)
  self._bDspBlpRdr = MrxUtil.SetDefault(tConfig.bDspBlpRdr, self._bDspBlpRdr)
  self._bDspBlpPda = MrxUtil.SetDefault(tConfig.bDspBlpPda, self._bDspBlpPda)
  self._bDspBlpWld = MrxUtil.SetDefault(tConfig.bDspBlpWld, self._bDspBlpWld)
end

function _ToggleMsgDisplay(self, bOn)
  local tConfig = {
    bDspMsgAdd = bOn,
    bDspMsgUpd = bOn,
    bDspMsgCpl = bOn,
    bDspMsgCcl = bOn
  }
  self:_SetMsgDisplay(tConfig)
end

function _SetMsgDisplay(self, tConfig)
  self._bDspMsgAdd = MrxUtil.SetDefault(tConfig.bDspMsgAdd, self._bDspMsgAdd)
  self._bDspMsgUpd = MrxUtil.SetDefault(tConfig.bDspMsgUpd, self._bDspMsgUpd)
  self._bDspMsgCpl = MrxUtil.SetDefault(tConfig.bDspMsgCpl, self._bDspMsgCpl)
  self._bDspMsgCcl = MrxUtil.SetDefault(tConfig.bDspMsgCcl, self._bDspMsgCcl)
end

function _SetDescPdaDisplay(self, bDspDescPda)
  self._bDspDescPda = MrxUtil.SetDefault(bDspDescPda, self._bDspDescPda)
end

function _SetTargetDisplay(self, uGuid, bOn, bPulsate)
  local tGuids = self._tTargets
  if not tGuids[uGuid] then
    tGuids[uGuid] = {}
  end
  tGuids[uGuid].bSuppressDsp = not bOn
  self:_RefreshTargetDisplay(uGuid)
  if bOn and bPulsate then
    PulsateRadarBlip(uGuid, 2)
  end
end

function _SetAllTargetsDisplay(self, bOn)
  local tGuids = self._tTargets
  for uGuid, tData in pairs(tGuids) do
    _SetTargetDisplay(self, uGuid, bOn)
  end
end

function _RefreshTargetDisplay(self, uGuid)
  local tTgtDspInfo = self._tTargets[uGuid]
  if not tTgtDspInfo then
    return false
  end
  local sGuidString = Sys.GuidToString(uGuid)
  local bShouldDisplay = tTgtDspInfo.bStatus and not tTgtDspInfo.bSuppressDsp and not tTgtDspInfo.bLimitedDsp
  local bBlipEnabled = self._bDspBlpRdr or self._bDspBlpPda or self._bDspBlpWld
  if not tTgtDspInfo.bDisplay and bShouldDisplay and bBlipEnabled then
    Debug.Printf("@@@@@@@@@ ADDED OBJECTIVE \"" .. self:GetName() .. "\", target " .. sGuidString)
    local tConfig = self:GetConfig()
    local tObjArgs = {}
    if self._bDspBlpRdr then
      tObjArgs = self:_BuildRadarBlipConfig(uGuid)
      Hud.Radar:AddObjective(tObjArgs)
    end
    local tWldArgs = {}
    if self._bDspBlpWld then
      local r, g, b = self._GetTargetBlipColor(tConfig.bOptional)
      local nVerticalOffset = 0
      if Object.HasLabel(uGuid, "Human") or Object.GetParent(uGuid) == Pg.GetGuidByName("location") then
        nVerticalOffset = 2
      end
      local sDspBlpWldIcon = self._GetTargetGameSpaceIcon() or "HUD_objective_action"
      if tTgtDspInfo.sType == "destination" then
        sDspBlpWldIcon = self._GetDestinationGameSpaceIcon()
      elseif tConfig.sDspBlpWldIcon then
        sDspBlpWldIcon = tConfig.sDspBlpWldIcon
      end
      local sMissionName
      local uiMissionNameHash = 0
      local oAncestorMission = self:GetMissionAncestor()
      if oAncestorMission and oAncestorMission:IsContract() then
        sMissionName = oAncestorMission:GetMissionId()
        if sMissionName then
          uiMissionNameHash = String.GetHash(sMissionName)
        end
      end
      local bJustCheck2D = self._GetJust2DCheckNeeded()
      local nNearDist = tConfig.nDspBlpWldNearDist or 5
      local nFarDist = tConfig.nDspBlpWldFarDist or 175
      tTgtDspInfo.uMarkerGuid = Marker.AddBlip(uGuid, sDspBlpWldIcon, 32, r, g, b, 255, nVerticalOffset, nNearDist, nFarDist, _knMarkerYClampDistance, sMissionName, bJustCheck2D)
      if Net.IsServer() then
        Net.SendEvent_AddMarkerObjective(uGuid, tTgtDspInfo.uMarkerGuid, r, g, b, nVerticalOffset, MrxUtil.MarkerGetIndexByName_World(sDspBlpWldIcon or ""), 1, 16, false, nNearDist, nFarDist, uiMissionNameHash)
      end
      tWldArgs.uMarkerGuid = tTgtDspInfo.uMarkerGuid
      tWldArgs.nR = r
      tWldArgs.nG = g
      tWldArgs.nB = b
      tWldArgs.nVerticalOffset = nVerticalOffset
      tWldArgs.sDspBlpWldIcon = sDspBlpWldIcon
    end
    local tPdaArgs = {}
    if self._bDspBlpPda and not tTgtDspInfo.bLimitedDsp then
      tPdaArgs = self:_BuildPdaBlipConfig(uGuid)
      Pda.Map:AddBlip(tPdaArgs)
    end
    tTgtDspInfo.bDisplay = true
    return true
  elseif tTgtDspInfo.bDisplay and bShouldDisplay and bBlipEnabled then
    Debug.Printf("@@@@@@@@@ UPDATING DISPLAY FOR OBJECTIVE \"" .. self:GetName() .. "\", target " .. sGuidString)
    if self._bDspBlpRdr then
      local tBlipConfig = self:_BuildRadarBlipConfig(uGuid)
      Hud.Radar:UpdateObjective(tBlipConfig)
    end
    if self._bDspBlpPda then
      local tBlipConfig = self:_BuildPdaBlipConfig(uGuid)
      Pda.Map:AddBlip(tBlipConfig)
    end
    return true
  elseif tTgtDspInfo.bDisplay and (not bShouldDisplay or not self._bDspBlpWld) then
    Debug.Printf("@@@@@@@@@ REMOVING DISPLAY FOR OBJECTIVE \"" .. self:GetName() .. "\", target " .. sGuidString)
    Hud.Radar:RemoveObjective({sName = sGuidString})
    if type(tTgtDspInfo.uMarkerGuid) == "userdata" then
      Marker.Remove(tTgtDspInfo.uMarkerGuid)
      if Net.IsServer() then
        Net.SendEvent_RemoveMarkerObjective(tTgtDspInfo.uMarkerGuid)
      end
    end
    if not tTgtDspInfo.bLimitedDsp then
      Pda.Map:RemoveBlip({sName = sGuidString})
    end
    tTgtDspInfo.bDisplay = false
    return true
  end
  return false
end

function _BuildRadarBlipConfig(self, uGuid)
  local tConfig = self:GetConfig()
  local r, g, b = self._GetTargetBlipColor(tConfig.bOptional)
  local tTgtDspInfo = self._tTargets[uGuid]
  if not tTgtDspInfo then
    return
  end
  local sDspBlpRdrIcon = self._GetTargetRadarIcon()
  if tTgtDspInfo.sType == "destination" then
    sDspBlpRdrIcon = self._GetDestinationRadarIcon()
  end
  if tConfig.sDspBlpRdrIcon then
    sDspBlpRdrIcon = tConfig.sDspBlpRdrIcon
  end
  local sTexture, nWidth, nHeight
  if sDspBlpRdrIcon then
    sTexture = sDspBlpRdrIcon
    nWidth = 10.666667
    nHeight = 10.666667
  else
    sTexture = "objective_action"
    nWidth = 8
    nHeight = 8
  end
  local nSortOrder = 5
  if tConfig.nSortOrder then
    nSortOrder = tConfig.nSortOrder
  elseif tConfig.bOptional then
    nSortOrder = 6
  end
  return {
    sName = Sys.GuidToString(uGuid),
    uGuid = uGuid,
    sTexture = sTexture,
    nR = r,
    nG = g,
    nB = b,
    nWidth = nWidth,
    nHeight = nHeight,
    bSticky = self._bDspBlpSticky,
    nSortOrder = nSortOrder
  }
end

function _BuildPdaBlipConfig(self, uGuid)
  local tTgtDspInfo = self._tTargets[uGuid]
  if not tTgtDspInfo then
    return
  end
  local tConfig = self:GetConfig()
  local sTexture = tConfig.sDspBlpPdaIcon or self._GetTargetPdaIcon(tConfig.bOptional) or "icon_yellow_mc"
  if tConfig.sDspBlpPdaIcon then
    sTexture = tConfig.sDspBlpPdaIcon
  end
  if tTgtDspInfo.sType == "destination" then
    sTexture = self._GetDestinationPdaIcon(tConfig.bOptional)
  end
  local oAncestorMission = self:GetMissionAncestor()
  local sMissionName
  if oAncestorMission then
    sMissionName = oAncestorMission:GetMissionId()
  end
  local nSortOrder = 2
  if tConfig.bOptional then
    nSortOrder = 3
  end
  return {
    sMission = sMissionName,
    sName = Sys.GuidToString(uGuid),
    sLabel = self:GetDescription(),
    uGuid = uGuid,
    sTexture = sTexture,
    nSortOrder = nSortOrder
  }
end

function _compare(a, b)
  return a[2] < b[2]
end

function _RefreshAllTargetDisplay(self)
  if not self._uTargetOverflow then
    for uGuid, tTgtDspInfo in pairs(self._tTargets) do
      self:_RefreshTargetDisplay(uGuid)
    end
  else
  end
end

function RefreshPdaDisplay(self)
  if self._bDspBlpPda then
    for uGuid, tTgtDspInfo in pairs(self._tTargets) do
      if tTgtDspInfo.bDisplay and tTgtDspInfo.bStatus and not tTgtDspInfo.bSuppressDsp then
        local tBlipConfig = self:_BuildPdaBlipConfig(uGuid)
        Pda.Map:AddBlip(tBlipConfig)
      end
    end
  end
end

function PulsateRadarBlips(self, nDuration)
  for uGuid, tTgtDspInfo in pairs(self._tTargets) do
    PulsateRadarBlip(uGuid, nDuration)
  end
end

function PulsateRadarBlip(uGuid, nDuration)
  local sGuidString = Sys.GuidToString(uGuid)
  Hud.Radar:AnimateObjectiveSize({
    sName = sGuidString,
    nMaxWidth = 12,
    nMaxHeight = 12,
    nSpeedWidth = 20,
    nSpeedHeight = 20,
    nDuration = nDuration
  })
end

function EnableTracking(self, bEnable)
  self._bDspBlpSticky = bEnable
  self:_RefreshAllTargetDisplay()
  if bEnable then
    self:PulsateRadarBlips()
    local sMsgType = "add"
    local tConfig = self:GetConfig()
    if tConfig.bDspBounty then
      sMsgType = "bty_add"
    elseif tConfig.bOptional then
      sMsgType = "bonus_add"
    end
    self:_PrintObjectiveMessage(sMsgType)
  end
end

function _PrintObjectiveMessage(self, sMsgType, fCallback, tCallbackArgs)
  local bDisplay
  local sDesc = GetObjectiveDescription(self:GetShortDescription(), self:GetProgressCompleted(), self:GetProgressQuota(), sMsgType)
  if sMsgType == "add" or sMsgType == "bonus_add" or sMsgType == "bty_add" then
    bDisplay = self._bDspMsgAdd
  elseif sMsgType == "upd" or sMsgType == "bonus_upd" or sMsgType == "bty_upd" or sMsgType == "collectible_upd" then
    bDisplay = self._bDspMsgUpd
  elseif sMsgType == "cpl" or sMsgType == "bonus_cpl" or sMsgType == "bty_cpl" then
    bDisplay = self._bDspMsgCpl
  elseif sMsgType == "ccl" or sMsgType == "bonus_ccl" or sMsgType == "bty_ccl" then
    bDisplay = self._bDspMsgCcl
    sDesc = self:GetShortDescription()
  end
  local sMsgGroupId = ""
  local oMission = self:GetMissionAncestor()
  if oMission then
    sMissionId = oMission:GetMissionId()
  end
  if sMissionId then
    sMsgGroupId = sMsgGroupId .. sMissionId
  end
  local sObjectiveId = self:GetName()
  if sObjectiveId then
    sMsgGroupId = sMsgGroupId .. sObjectiveId
  end
  MrxGuiInterface.DisplayObjectiveMessage(bDisplay, self:GetInlineIcon(), sMsgType, sDesc, sMsgGroupId, fCallback, tCallbackArgs)
end

function GetDisplayDescription(self)
  return self._bDspDescPda
end

function GetDescription(self, bPrependInlineIcon)
  local sReturn = GetObjectiveDescription(self:GetShortDescription(), self._nCompleted, self._nQuota)
  if bPrependInlineIcon then
    local sInlineIcon = self:GetInlineIcon()
    if sInlineIcon then
      sReturn = sInlineIcon .. " " .. sReturn
    end
  end
  return sReturn
end

function GetObjectiveDescription(sObjDesc, nCompleted, nQuota, sMsgType)
  local sReturnStr = sObjDesc
  nCompleted = nCompleted or 0
  nQuota = nQuota or 0
  local sProgress
  if nQuota <= 0 and 1 <= nCompleted then
    sProgress = "(" .. nCompleted .. ")"
  elseif 1 < nQuota then
    sProgress = "(" .. nCompleted .. "/" .. nQuota .. ")"
  end
  Debug.Printf("GetObjectiveDescription sMsgType: " .. tostring(sMsgType))
  if sObjDesc and sProgress then
    if sMsgType == "collectible_upd" then
      sReturnStr = sProgress .. ": " .. sObjDesc
    else
      sReturnStr = sObjDesc .. " " .. sProgress
    end
  end
  return sReturnStr
end

function GetShortDescription(self)
  local tConfig = self:GetConfig()
  return MrxUtil.SetDefault(tConfig.sDspShortDesc, self:_GetShortDescription())
end

function GetProgressQuota(self)
  return self._nQuota
end

function GetProgressCompleted(self)
  return self._nCompleted
end

function GetMissionAncestor(self)
  local oCurrentTask = self
  while true do
    local oParent = oCurrentTask:GetParent()
    if oParent then
      if oParent.GetMissionId then
        return oParent
      end
      oCurrentTask = oParent
    else
      break
    end
  end
end

function _UpdateMissionInPda(self)
  local oMission = self:GetMissionAncestor()
  oMission:RefreshPdaDisplay()
end

function _GetShortDescription()
  return "NULL"
end

function _GetTargetBlipColor(bOptional)
  if bOptional then
    return MrxUtil.GetSecondaryObjectiveRgb()
  else
    return MrxUtil.GetPrimaryObjectiveRgb()
  end
end

function _GetJust2DCheckNeeded()
  return false
end

function GetInlineIcon(self)
  local tConfig = self:GetConfig()
  if tConfig.bOptional then
    return "[objaction2]"
  else
    return "[objaction]"
  end
end

function _GetTargetRadarIcon()
  return nil
end

function _GetTargetPdaIcon(bOptional)
  if bOptional then
    return "icon_action_2_mc"
  else
    return "icon_action_1_mc"
  end
end

function _GetTargetGameSpaceIcon()
  return nil
end

function _IsValidTarget(uGuid)
  return true
end

function DisplayTextInSatelliteMode(tObjectives, uGuid)
  if not uGuid then
    return
  end
  for k, oObjective in pairs(tObjectives) do
    if oObjective._tTargets and oObjective._tTargets[uGuid] then
      local tConfig = oObjective:GetConfig()
      if tConfig.bOptional then
        return "[2ndobjt]" .. oObjective:GetDescription(true)
      else
        return "[objt]" .. oObjective:GetDescription(true)
      end
    end
  end
end
