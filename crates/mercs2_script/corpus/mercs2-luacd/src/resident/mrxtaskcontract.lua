inherit("MrxTaskMission")
import("MrxFactionManager")
import("MrxGui")
import("MrxHqManager")
import("MrxPlayer")
import("MrxPlayState")
import("WifPmcInterior")
import("MrxSoundCategories")
import("MrxActionHijack")
import("MrxMusic")
import("MrxState")
import("MrxStatsManager")
import("MrxParkingLotManager")
import("MrxGuiInterface")

function OnPlayerJoined(self, iPlayerId, uPlayerGuid, uCharacterGuid)
  Debug.Printf("MrxTaskContract:OnPlayerJoined ", iPlayerId, " ", Sys.GuidToString(uPlayerGuid), " ", Sys.GuidToString(uCharacterGuid))
end

function OnPlayerLeft(self, iPlayerId, uPlayerGuid, uCharacterGuid)
  Debug.Printf("MrxTaskContract:OnPlayerLeft ", iPlayerId, " ", Sys.GuidToString(uPlayerGuid), " ", Sys.GuidToString(uCharacterGuid))
end

function PreLoadAssets(self)
  local tSaveData = self:_GetSaveData()
  if type(tSaveData) == "table" then
    self._tContractState = tSaveData.tContractState or {}
  else
    self._tContractState = {}
  end
end

function AssetsLoaded(self)
  if Net.DoneReloadingLayers then
    Net.DoneReloadingLayers()
  end
  MrxState.AddGlobalExitCallback(self.Activated, {self})
  self:_IssueAssetsLoadedCallbacks()
end

function Activated(self)
  local tConfig = self:GetConfig()
  if tConfig.tRewards and (tConfig.tRewards.nWager or tConfig.tRewards.nWagerPercent) then
    local oPauseMenu = MrxGui.GetWidgetByName("Pause Layout")
    if oPauseMenu then
      oPauseMenu:SetUserSaveEnabled(false)
    end
  end
  MrxPlayState.Set(MrxPlayState._knMission)
  MrxPlayState.SetCurrentMission(self)
  if not tConfig.bSuppressPdaDisplay then
    local sMissionName = self:GetMissionId()
    Pda.Map:SetSelectedMission({sName = sMissionName})
  end
  local tRetryLocations = WifMissionFlow.GetRetryLocations()
  if not tRetryLocations then
    WifMissionFlow.SetRetryLocations(self:GetStartLocations())
  end
  MrxTaskMission.Activated(self)
  local tConfig = self:GetConfig()
  local tMappings = {
    All = "an",
    Chi = "ch",
    Gur = "gr",
    Oil = "oc",
    Pmc = "pmc",
    Vza = "pmc"
  }
  local sKey = tMappings[tConfig.sFactionId]
  if sKey then
    MrxMusic.EnterContractMusic(sKey)
  end
  local uGuid = Player.GetLocalCharacter()
  if uGuid then
    Object.SetHealth(uGuid, Object.GetMaxHealth(uGuid))
  end
  _Checkpoint(nil, true, true)
  local tPlayers = Player.GetAllPlayers()
  for i, uPlayerGuid in ipairs(tPlayers) do
    local uCharGuid = Player.GetCharacter(uPlayerGuid)
    if uCharGuid and Player.IsRemote(i) then
      self:OnPlayerJoined(i, uPlayerGuid, uCharGuid)
    end
  end
  if MrxFactionManager.IsAttitudeMutable(tConfig.sFactionId) then
    self._uFactionAttitudeChanged = MrxFactionManager.CreateAttitudeChangeEvent({
      tConfig.sFactionId,
      "Pmc",
      nil,
      "Hostile"
    }, function()
      self:_SetCancelMessage("[Fanfare.Cancel.FactionHostile]")
      self:Cancel()
    end)
  end
  local sFactionTemplate = MrxFactionManager.GetFactionTemplateName(tConfig.sFactionId)
  Ai.SetInfractionMultiplier(Pg.GetGuidByName(sFactionTemplate), 0.2)
  local sMissionName = self:GetMissionId()
  Pg.ContractActivated(sMissionName)
  MrxParkingLotManager.MarkLastVehicle()
end

function Complete(self)
  if self._bEndSequenceInProgress then
    return
  end
  self._bEndSequenceInProgress = true
  Debug.Printf(" We are starting End Sequence, Setting ActionHijack false")
  local uPlayerGuid = Player.GetLocalPlayer()
  if uPlayerGuid then
    Player.SetSeatMovementLocks(uPlayerGuid, false)
  end
  if MrxActionHijack.IsInHijack() and self:GetMissionId() ~= "PmcCon004" then
    Debug.Printf("In Hijack - Complete paused ")
    MrxActionHijack.SetUnloadCallback(self.Complete1, {self})
  else
    Debug.Printf("Not in Hijack")
    self:Complete1()
  end
  Pg.ContractCompleted()
end

function Complete1(self)
  Debug.Printf("Continue complete")
  if MrxHqManager.IsInside() then
    MrxHqManager.SetUnloadCallback(self.Complete2, {self})
  elseif WifPmcInterior.IsInside() then
    WifPmcInterior.SetUnloadCallback(self.Complete2, {self})
  else
    self:Complete2()
  end
end

function Complete2(self)
  if self:IsCompleted() then
    Debug.Printf("Completion of task " .. self:GetName() .. " FAILED; task is already completed.")
    return
  end
  if self:GetMissionId() == "OilCon002" then
    Net.SendCustomEvent("MrxTaskContract", NETEVENT_CLIENTPAUSE, {})
  end
  Debug.Printf("Task \"" .. self:GetLineage() .. "\" complete")
  self:_SetState(MrxTaskState._knCompleted)
  WifMissionFlow.SetRetryLocations(nil)
  MrxMusic.PlayFanfare(true)
  local sPlayer1Name = Object.GetLocalizedName(Player.GetPrimaryCharacter())
  local sPlayer2Name
  local uPlayer2Guid = Player.GetSecondaryCharacter()
  if uPlayer2Guid then
    sPlayer2Name = Object.GetLocalizedName(uPlayer2Guid)
  end
  _SetPlayersInvincible(true)
  local tConfig = self:GetConfig()
  local sType
  if tConfig.bPlayerVisibleMission then
    sType = "mission"
  elseif tConfig.tRewards and (tConfig.tRewards.nWager or tConfig.tRewards.nWagerPercent) then
    sType = "wager"
  else
    sType = "contract"
  end
  local i = 1
  Hud.Fanfare:Create({
    sType = sType,
    sProfileName1 = sPlayer1Name,
    sProfileName2 = sPlayer2Name,
    fCallback = function(bRepeat)
      if 1 < i then
        return
      end
      i = i + 1
      MrxSoundCategories.Fade("fanfare", false)
      _SetPlayersInvincible(false)
      self:Cleanup()
      self:_IssueStateChangeCallbacks()
    end
  })
  if sType == "contract" then
    local nContractFee = 0
    if self._nContractReward then
      nContractFee = self._nContractReward
    elseif tConfig.tRewards and tConfig.tRewards.nCash then
      nContractFee = tConfig.tRewards.nCash
    end
    local nBonus1 = 0
    local nBonus2 = 0
    if self._nBonus1 then
      nBonus1 = self._nBonus1
    end
    if self._nBonus2 then
      nBonus2 = self._nBonus2
    end
    local tLedgerItems = {
      {
        sDescription = "[Fanfare.Completion.ContractFee]",
        sValueType = "$",
        nValue = nContractFee
      },
      {
        sDescription = "[Fanfare.Completion.Bonus]",
        sValueType = "$",
        nValue = nBonus1
      },
      {
        sDescription = "[Fanfare.Completion.Total]",
        sValueType = "$",
        nValue = nContractFee + nBonus1
      }
    }
    for i, tLedgerItem in ipairs(tLedgerItems) do
      tLedgerItem.nPlayer = 1
      Hud.Fanfare:AddItem(tLedgerItem)
    end
    if uPlayer2Guid then
      tLedgerItems[2].nValue = nBonus2
      tLedgerItems[3].nValue = nContractFee + nBonus2
      for i, tLedgerItem in ipairs(tLedgerItems) do
        tLedgerItem.nPlayer = 2
        Hud.Fanfare:AddItem(tLedgerItem)
      end
    end
    if not tConfig.tRewards then
      tConfig.tRewards = {}
    end
    tConfig.tRewards.nCashOverride = nContractFee + nBonus1
    if uPlayer2Guid then
      tConfig.tRewards.nCashOverride2 = nContractFee + nBonus2
    end
  elseif sType == "wager" then
    local nWinnings = 0
    if tConfig.tRewards and tConfig.tRewards.nWagered then
      nWinnings = nWinnings + tConfig.tRewards.nWagered
    end
    local tLedgerItem = {
      nPlayer = 1,
      sDescription = "[Fanfare.Wager.Winnings]",
      sValueType = "$",
      nValue = nWinnings
    }
    Hud.Fanfare:AddItem(tLedgerItem)
    if uPlayer2Guid then
      tLedgerItem.nPlayer = 2
      tLedgerItem.nValue = 0
      Hud.Fanfare:AddItem(tLedgerItem)
    end
  end
  MrxSoundCategories.Fade("fanfare", true)
  Hud.Fanfare:Commence({})
  Debug.Printf("FanFare Begining")
end

function Cancel(self)
  if self._bEndSequenceInProgress then
    return
  end
  self._bEndSequenceInProgress = true
  local uPlayerGuid = Player.GetLocalPlayer()
  if uPlayerGuid then
    Player.SetSeatMovementLocks(uPlayerGuid, false)
  end
  Debug.Printf(" We are starting End Sequence, Setting ActionHijack false")
  if MrxHqManager.IsInside() then
    MrxHqManager.SetUnloadCallback(self.Cancel2, {self})
  elseif WifPmcInterior.IsInside() then
    WifPmcInterior.SetUnloadCallback(self.Cancel2, {self})
  else
    self:Cancel2()
  end
  if Player.ClearGPS then
    local uPlayer = Player.GetLocalPlayer()
    if uPlayer then
      Player.ClearGPS(uPlayer)
    end
  end
  Pg.ContractCancelled()
end

function Cancel2(self)
  if self:IsCancelled() then
    Debug.Printf("Cancellation of task " .. self:GetName() .. " FAILED; task is already cancelled.")
    return
  end
  Debug.Printf("Task \"" .. self:GetLineage() .. "\" cancelled")
  self:_SetState(MrxTaskState._knCancelled)
  MrxMusic.PlayFanfare(false)
  local tConfig = self:GetConfig()
  local sType
  if tConfig.bPlayerVisibleMission then
    sType = "mission"
  elseif tConfig.tRewards and (tConfig.tRewards.nWager or tConfig.tRewards.nWagerPercent) then
    sType = "wager"
  else
    sType = "contract"
  end
  local bRetryable = sType ~= "wager" and tConfig.sStarter
  local i = 1
  if not self._sCancelMsg then
    local tGenericCancelLines = {
      "[Generic.Failures.002]",
      "[Generic.Failures.003]",
      "[Generic.Failures.005]",
      "[Generic.Failures.009]",
      "[Generic.Failures.011]"
    }
    self._sCancelMsg = MrxUtil.GetRandomTableElement(tGenericCancelLines)
  end
  Hud.Fanfare:Create({
    sType = sType,
    sProfileName1 = "unused",
    bAllowRetry = bRetryable,
    sCancelMsg = self._sCancelMsg,
    fCallback = function(bRetry)
      if 1 < i then
        return
      end
      i = i + 1
      MrxSoundCategories.Fade("fanfare", false)
      _SetPlayersInvincible(false)
      if bRetry ~= nil then
        bRetry = not bRetry
        bRetry = bRetryable and bRetry
      end
      if not tConfig.sStarter then
        bRetry = true
      end
      self:_DialogBoxDismissed(bRetry)
    end
  })
  MrxSoundCategories.Fade("fanfare", true)
  _SetPlayersInvincible(true)
  Hud.Fanfare:Commence({})
end

function Cleanup(self)
  self._bEndSequenceInProgress = nil
  Debug.Printf(" We are ending End Sequence, Setting ActionHijack true")
  local uPlayerGuid = Player.GetLocalPlayer()
  if uPlayerGuid then
    Player.SetSeatMovementLocks(uPlayerGuid, true)
  end
  if self._uFactionAttitudeChanged then
    Event.Delete(self._uFactionAttitudeChanged)
    self._uFactionAttitudeChanged = nil
  end
  MrxVoSequence.Stop(nil, false, MrxVoSequence.knPriorityContract)
  local tConfig = self:GetConfig()
  local sFactionTemplate = MrxFactionManager.GetFactionTemplateName(tConfig.sFactionId)
  Ai.SetInfractionMultiplier(Pg.GetGuidByName(sFactionTemplate), 1)
  if not tConfig.bSuppressPdaDisplay then
    Pda.Map:SetSelectedMission({sName = nil})
  end
  Hud.MessageBox:Clear({})
  MrxTaskMission.Cleanup(self)
  if tConfig.tRewards and (tConfig.tRewards.nWager or tConfig.tRewards.nWagerPercent) then
    local oPauseMenu = MrxGui.GetWidgetByName("Pause Layout")
    if oPauseMenu then
      Debug.Printf("Enabling pause")
      oPauseMenu:SetUserSaveEnabled(true)
    end
  end
end

function _DialogBoxDismissed(self, bRetry)
  self.bRetry = bRetry
  self:Cleanup()
  self:_IssueStateChangeCallbacks()
  if bRetry then
    MrxStatsManager.IncreaseRetriesCounter()
    MrxLayerManager.ProcessMarkedLayers(Pg.LoadGame, {"retry"})
  else
    self:GetParent():Cancel()
    local tConfig = self:GetConfig()
    if (not tConfig.tRewards or not tConfig.tRewards.nWager and not tConfig.tRewards.nWagerPercent) and WifPmcInterior.IsUnlocked() then
      if not MrxPlayer.AreAnyHeroesAlive() then
        MrxPlayer.MoveToSickbay()
      elseif not Net.IsClient() then
        local uLocalChar = Player.GetLocalCharacter()
        if not Object.IsAlive(uLocalChar) and Player.IsBoundaryDeath(uLocalChar) then
          MrxPlayer.MoveToSickbay()
        end
      end
    end
    if self._bCancelByMedEvac then
      MrxPlayer.MoveToSickbay()
    end
    WifMissionFlow.SetRetryLocations(nil)
  end
  self.bRetry = nil
end

function _GetMissionType()
  return MrxTaskMission._knContract
end

function IsContract()
  return true
end

function SetCancelByMedEvac(self, bSet)
  self._bCancelByMedEvac = bSet
end

function _SetFlag(self, sFlagName, vFlagValue)
  if type(sFlagName) == "string" then
    vFlagValue = vFlagValue or 1
    if self._tContractState then
      self._tContractState[sFlagName] = vFlagValue
    end
  end
end

function _GetFlag(self, sFlagName)
  if self._tContractState then
    return self._tContractState[sFlagName]
  end
end

function _Checkpoint(tSpawnLocations, bNoAutosave, bHideMessages)
  WifMissionFlow.EnableCheckpointSaveMode(true)
  if tSpawnLocations then
    WifMissionFlow.SetRetryLocations(tSpawnLocations)
  end
  Pg.SaveGame("retry")
  WifMissionFlow.EnableCheckpointSaveMode(false)
  if not bNoAutosave then
    WifMissionFlow.Autosave()
  end
  if not bHideMessages then
    Hud.MessageBox:AddMessage({
      sMessage = "[Generic.CheckpointReached]"
    })
    if Net.IsServer() then
      Net.SendEvent_ObjectiveMessage(99, "", "")
    end
  end
end

function SaveInstance(self, bDefaultState)
  local tSaveData = MrxTaskMission.SaveInstance(self)
  if bDefaultState then
    tSaveData.tContractState = nil
  else
    tSaveData.tContractState = self._tContractState
  end
  return tSaveData
end

function _SetCancelMessage(self, sCancelMsg)
  self._sCancelMsg = sCancelMsg
end

function _SetContractReward(self, nContractReward)
  self._nContractReward = nContractReward
end

function _SetPlayer1Bonus(self, nBonus1)
  self._nBonus1 = nBonus1
end

function _SetPlayer2Bonus(self, nBonus2)
  self._nBonus2 = nBonus2
end

function _SetPlayersInvincible(bSet)
  local uPlayer1Guid = Player.GetPrimaryCharacter()
  if uPlayer1Guid then
    Object.SetInvincible(uPlayer1Guid, bSet, "Fanfare")
  end
  local uPlayer2Guid = Player.GetSecondaryCharacter()
  if uPlayer2Guid then
    Object.SetInvincible(uPlayer2Guid, bSet, "Fanfare")
  end
end

NETEVENT_CLIENTPAUSE = 0

function NetEventCallback(nEventType)
  if nEventType == NETEVENT_CLIENTPAUSE then
    MrxGuiInterface.HudInterface.FanfareQueue.ClientPause(true)
    MrxGuiInterface.HudInterface.FanfareQueue.ClientSetPending(false)
  end
end
