import("MrxFactionManager")
import("MrxGui")
import("MrxGuiBootstrap")
import("MrxHqManager")
import("MrxLayerManager")
import("MrxStarterManager")
import("MrxUtil")
import("MrxTransit")
import("MrxPlayState")
import("WifFreePlay")
import("WifMissionData")
import("WifMissionFlow")
import("WifVzBoundary")
import("MrxSound")
import("WifHqData")
import("MrxState")
import("MrxTutorialManager")
_tAssetPreload = {
  animation = {
    Chris = {
      "player_chris_job_briefing_greeting",
      "player_chris_job_briefing_idle",
      "player_chris_job_briefing_no",
      "player_chris_job_briefing_yes",
      "player_chris_job_briefing_spiel"
    },
    Jennifer = {
      "player_jennifer_job_briefing_greeting",
      "player_jennifer_job_briefing_idle",
      "player_jennifer_job_briefing_no",
      "player_jennifer_job_briefing_yes",
      "player_jennifer_job_briefing_spiel"
    },
    Mattias = {
      "player_mattias_job_briefing_greeting_fb",
      "player_mattias_job_briefing_idle_fb",
      "player_mattias_job_briefing_no_fb",
      "player_mattias_job_briefing_yes_fb",
      "player_mattias_job_briefing_spiel_fb"
    },
    MaleStarter = {
      "all_starter02_job_briefing_idle",
      "all_starter02_job_briefing_greeting_neutral",
      "all_starter02_job_briefing_greeting_happy",
      "all_starter02_job_briefing_greeting_angry",
      "all_starter02_job_briefing_spiel",
      "all_starter02_job_briefing_goodbye"
    },
    FemaleStarter = {
      "all_starter03_job_briefing_idle",
      "all_starter03_job_briefing_greeting_neutral",
      "all_starter03_job_briefing_greeting_happy",
      "all_starter03_job_briefing_greeting_angry",
      "all_starter03_job_briefing_spiel",
      "all_starter03_job_briefing_goodbye"
    }
  },
  facefxanimationset = {
    Chris = {
      "Global_Job_Briefing_Chris"
    },
    Jennifer = {
      "Global_Job_Briefing_Jennifer"
    },
    Mattias = {
      "Global_Job_Briefing_Mattias"
    }
  },
  wavebank = {
    "vo_job_heros"
  },
  soundbank = {
    "vo_job_heros"
  }
}
_NetSafeBriefingModule = nil

function NetSafeBriefingModuleLoaded(mModule)
  GlobalEnter(false)
  _NetSafeBriefingModule = mModule
  _NetSafeBriefingModule.NetSafeLoadBriefingAssets(_tAssetPreload)
  MrxState.Exit(MrxState.STATE_WAITFORGAME)
end

function NetSafeLoadAssets1()
  dynamic_import("MrxBriefing", NetSafeBriefingModuleLoaded)
end

function NetSafeLoadAssets()
  MrxState.Enter(MrxState.STATE_WAITFORGAME, NetSafeLoadAssets1)
end

function NetSafeUnloadAssets()
  if _NetSafeBriefingModule then
    GlobalExit(false)
    _NetSafeBriefingModule.NetSafeUnloadBriefingAssets(_tAssetPreload)
    dynamic_remove("MrxBriefing")
    _NetSafeBriefingModule = nil
  end
end

function Create(mModule, self)
  self = self or {}
  setmetatable(self, {__index = mModule})
  self._bLocked = true
  self._bInside = false
  self._tStarter = nil
  return self
end

function SetName(self, sName)
  self._sName = sName
end

function GetName(self)
  return self._sName
end

function SetLock(self, bLocked)
  self._bLocked = bLocked
end

function IsLocked(self)
  return self._bLocked
end

function GetRadarIcon(self)
  return self.sRadarIcon
end

function GetAtmosphere(self)
  return self.sAtmosphere
end

function GetDrawDistance(self)
  return self.nDrawDistance or 50
end

function GetEntryLocations(self)
  return {
    self.tPortal.sStart1,
    self.tPortal.sStart2
  }
end

function GetBuildingName(self)
  return self.sBuildingName
end

function GetFaction(self)
  if self._tStarter then
    return self._tStarter:GetFaction()
  end
end

function SetRespawn(self, bEnable)
  self._bRespawn = bEnable
end

function GetRespawn(self)
  return self._bRespawn
end

function NetSafeAddHqPdaBlip(nHqIndex, bSticky, nFactionIndex, sBlipLabel, tMissions, bStarterIsBoss, bUnlocked)
  local sHqName = WifHqData.GetHqIdFromIndex(nHqIndex)
  local tHqConfig = WifHqData.GetHqConfigFromId(sHqName)
  local sBlipName = sHqName .. "::Portal"
  local uBlippedObject = Pg.GetGuidByName(tHqConfig.tPortal.sEntrance)
  local sPdaTexture = ""
  if not bUnlocked then
    sPdaTexture = tHqConfig.sPdaIconLocked
  else
    sPdaTexture = tHqConfig.sPdaIcon
  end
  local sFaction = MrxFactionManager.GetFactionIdFromIndex(nFactionIndex)
  local nLandingZone = tHqConfig.nLandingZone
  AddHqPdaBlip(sBlipName, uBlippedObject, sPdaTexture, bSticky, sFaction, sBlipLabel, tMissions, bStarterIsBoss, nLandingZone)
end

function GetMissionDesc(tMissions)
  local sDesc = ""
  for _, sMissionName in pairs(tMissions) do
    local sTitle = WifMissionData.GetMissionTitle(sMissionName)
    local sFactionId = WifMissionData.GetMissionFaction(sMissionName)
    local sInlineIcon = ""
    if sFactionId then
      sInlineIcon = MrxFactionManager.GetInlineIcon(sFactionId) .. " "
    end
    sDesc = sDesc .. sInlineIcon .. "\"" .. sTitle .. "\""
    if WifMissionData.GetMissionRepeatable(sMissionName) then
      local nCompleted = WifMissionFlow.GetKeyValue(sMissionName)
      local nLevel = nCompleted + 1
      local nLevels = WifMissionData.GetMissionLevels(sMissionName)
      if nLevel > nLevels then
        nLevel = nLevels
      end
      sDesc = sDesc .. " ([Generic.Level] " .. nLevel .. ")"
    end
    sDesc = sDesc .. "\n"
  end
  return sDesc
end

function AddHqPdaBlip(sBlipName, uBlippedObject, sPdaTexture, bSticky, sFaction, sBlipLabel, tMissions, bStarterIsBoss, nLandingZone, sLockStatusMessage)
  local sDesc = ""
  if not bStarterIsBoss then
    sDesc = sDesc .. "[cash] [Briefing.Shop]\n"
  end
  if nLandingZone and MrxTransit.IsLocationEnabled(nLandingZone) then
    sDesc = sDesc .. "[vehheli] [Briefing.Transit]\n"
  end
  if sDesc ~= "" then
    sDesc = sDesc .. "\n"
  end
  sDesc = sDesc .. "[PDA.Map.WorkAvailableHeader]\n"
  local nMissions = 0
  local tCriticalPathMission = {}
  local tNonCriticalPathMission = {}
  for _, nMissionIndex in pairs(tMissions) do
    local sMissionName = WifMissionData.GetMissionIdFromIndex(nMissionIndex)
    local bCriticalPath = WifMissionData.IsMissionOnCriticalPath(sMissionName)
    if bCriticalPath then
      table.insert(tCriticalPathMission, sMissionName)
    else
      table.insert(tNonCriticalPathMission, sMissionName)
    end
    nMissions = nMissions + 1
  end
  table.sort(tCriticalPathMission)
  table.sort(tNonCriticalPathMission)
  if nMissions <= 0 then
    sDesc = sDesc .. "([Generic.None])"
  else
    if 0 < table.getn(tCriticalPathMission) then
      sDesc = sDesc .. GetMissionDesc(tCriticalPathMission)
    end
    if 0 < table.getn(tNonCriticalPathMission) then
      sDesc = sDesc .. GetMissionDesc(tNonCriticalPathMission)
    end
  end
  if sLockStatusMessage then
    local sLockedMessage
    if sLockStatusMessage == "Friendly" then
      sLockedMessage = "[Generic.HQ.FriendlyPDA]"
    elseif sLockStatusMessage == "Neutral" then
      sLockedMessage = "[Generic.HQ.NeutralPDA]"
    elseif sLockStatusMessage == "NoEntry" then
      sLockedMessage = "[Generic.HQ.NoEntryPDA]"
    end
    if sLockedMessage then
      sDesc = sLockedMessage .. [[


]] .. sDesc
    end
  end
  local sPdaFaction = MrxFactionManager.GetPdaFactionIdFromFactionId(sFaction) or "PMC"
  Pda.Map:AddBlip({
    sName = sBlipName,
    uGuid = uBlippedObject,
    sLabel = sBlipLabel,
    sDesc = sDesc,
    sTexture = sPdaTexture,
    bSticky = bSticky,
    sMission = nil,
    sFaction = sPdaFaction,
    bTodoList = false,
    nSortOrder = 4,
    bDontNetSync = true
  })
end

function NetSafeRemoveHqPdaBlip(nHqIndex)
  local sHqName = WifHqData.GetHqIdFromIndex(nHqIndex)
  local sBlipName = sHqName .. "::Portal"
  RemoveHqPdaBlip(sBlipName)
end

function RemoveHqPdaBlip(sBlipName)
  Pda.Map:RemoveBlip({sName = sBlipName, bDontNetSync = true})
end

function RefreshUiDisplay(self)
  if Net.IsClient() then
    return
  end
  local bDisplay = false
  local bIsFreePlay = MrxPlayState.IsFree()
  local bInsideHq = self._bInside
  local bContentAvailable, bStarterIsBoss, bStarterHasCriticalPathBriefings
  local sRadarTexture = self.sRadarIcon
  local sFactionId = self:GetFaction()
  local bSufficientFactionAttitude = true
  local bUnlocked = true
  local sActionDisplay
  if self._tStarter then
    bContentAvailable = self._tStarter:_GetBriefingCount() > 0
    bStarterIsBoss = self._tStarter:IsBoss()
    bStarterHasCriticalPathBriefings = self._tStarter:HasCriticalPathBriefings()
    if MrxFactionManager.IsAttitudeMutable(sFactionId) then
      local sTargetAttitude = "Neutral"
      if bStarterIsBoss then
        sTargetAttitude = "Friendly"
      end
      bSufficientFactionAttitude = MrxFactionManager.TestAttitude(sFactionId, "Pmc", ">=", sTargetAttitude)
      bUnlocked = bUnlocked and bSufficientFactionAttitude
    end
    local bHasShop = self._tStarter:HasShop()
    bDisplay = not bInsideHq
    sActionDisplay = self._tStarter:GetActionDisplay()
    if bStarterIsBoss then
      bUnlocked = bUnlocked and bContentAvailable and bIsFreePlay
    else
      bUnlocked = bUnlocked and (bContentAvailable or bHasShop)
    end
  else
  end
  bDisplay = bDisplay and not self:IsLocked()
  if not bUnlocked then
    if bStarterIsBoss then
      if not bIsFreePlay then
        self.sLockStatusMessage = "NoEntry"
      elseif not bContentAvailable then
        self.sLockStatusMessage = "NoContract"
      else
        self.sLockStatusMessage = "Friendly"
      end
    else
      self.sLockStatusMessage = "Neutral"
    end
  else
    self.sLockStatusMessage = nil
  end
  self:SetPortal(bDisplay, sActionDisplay)
  local sBlipName = self:GetName() .. "::Portal"
  local nHqIndex = WifHqData.GetHqIndexFromId(self:GetName())
  if bDisplay then
    if not bUnlocked and self.sRadarIconLocked then
      sRadarTexture = self.sRadarIconLocked
    end
    local nSize = bStarterIsBoss and 8 or 6
    local bSticky = bIsFreePlay and bContentAvailable and bStarterHasCriticalPathBriefings
    local uBlippedObject = Pg.GetGuidByName(self.tPortal.sEntrance)
    Hud.Radar:AddObjective({
      sName = sBlipName,
      uGuid = uBlippedObject,
      nR = 255,
      nG = 255,
      nB = 255,
      nWidth = nSize,
      nHeight = nSize,
      sTexture = sRadarTexture,
      bSticky = bSticky,
      nSortOrder = bStarterIsBoss and 5 or 6
    })
    local tMissions = {}
    local numMissions = 0
    local tBriefings = self._tStarter:GetOfferedBriefings()
    for sMissionName, _ in pairs(tBriefings) do
      numMissions = numMissions + 1
      tMissions[numMissions] = WifMissionData.GetMissionIndexFromId(sMissionName)
    end
    local iSticky = 0
    if bSticky ~= nil then
      if bSticky == true then
        iSticky = 1
      else
        iSticky = 2
      end
    end
    local sPdaIcon = self.sPdaIcon
    if not bUnlocked and self.sPdaIconLocked then
      sPdaIcon = self.sPdaIconLocked
    end
    local sFaction = self:GetFaction()
    local nFactionIndex = MrxFactionManager.GetIndexFromFactionId(sFaction)
    local sBlipLabel = "UNNAMED"
    if self.sBlipLabel then
      sBlipLabel = self.sBlipLabel
    elseif not bStarterIsBoss and self.nLandingZone then
      sBlipLabel = MrxTransit.GetName(self.nLandingZone)
    end
    Net.SendEvent_AddHqPdaBlip(nHqIndex, iSticky, nFactionIndex, sBlipLabel, tMissions, bStarterIsBoss, bUnlocked)
    AddHqPdaBlip(sBlipName, uBlippedObject, sPdaIcon, bSticky, sFaction, sBlipLabel, tMissions, bStarterIsBoss, self.nLandingZone, self.sLockStatusMessage)
  else
    Hud.Radar:RemoveObjective({sName = sBlipName})
    Net.SendEvent_RemoveHqPdaBlip(nHqIndex)
    RemoveHqPdaBlip(sBlipName)
  end
end

function SetPortal(self, bEnable, sActionDisplay)
  if bEnable then
    Debug.Printf("@@@@@@@@@@ Enabling " .. self:GetName() .. " portal")
  else
    Debug.Printf("@@@@@@@@@@ Disabling " .. self:GetName() .. " portal")
  end
  local tPortalData = self.tPortal
  local sActionableName = tPortalData.sEntrance
  local uGuid = Pg.GetGuidByName(sActionableName)
  local bUnlocked = true
  if bEnable then
    local bSufficientFactionAttitude = true
    local sFactionId = self:GetFaction()
    local bStarterIsBoss = self._tStarter:IsBoss()
    if MrxFactionManager.IsAttitudeMutable(sFactionId) then
      local sTargetAttitude = "Neutral"
      if bStarterIsBoss then
        sTargetAttitude = "Friendly"
      end
      bSufficientFactionAttitude = MrxFactionManager.TestAttitude(sFactionId, "Pmc", ">=", sTargetAttitude)
      bUnlocked = bUnlocked and bSufficientFactionAttitude
    end
    Pg.RemoveContextAction(uGuid)
    if self._uEvent then
      Event.Delete(self._uEvent)
      self._uEvent = nil
    end
    HideTutorialMessage(self)
    if self._uMarker then
      Marker.Remove(self._uMarker)
      if Net.IsServer() then
        Net.SendEvent_RemoveMarkerObjective(self._uMarker)
      end
      self._uMarker = nil
    end
    local bContentAvailable = self._tStarter:_GetBriefingCount() > 0
    local bIsFreePlay = MrxPlayState.IsFree()
    local bHasShop = self._tStarter:HasShop()
    if bStarterIsBoss then
      bUnlocked = bUnlocked and bContentAvailable and bIsFreePlay
    else
      bUnlocked = bUnlocked and (bContentAvailable or bHasShop)
    end
    local sText = "[ContextAction.Enter]"
    if sActionDisplay then
      sText = sActionDisplay
    end
    local fCallBackFunc = self._OnEnter
    if not bUnlocked then
      sText = "[neut]" .. sText
      fCallBackFunc = self._LockedOnEnter
    end
    Pg.AddContextAction(uGuid, sText, 2, 0, 0, 255, 2, false)
    self._uEvent = Event.CreatePersistent(Event.ContextAction, {0, uGuid}, fCallBackFunc, {self})
    local tHudIcons, icon_r, icon_g, icon_b, disc_r, disc_g, disc_b
    if sFactionId == "Pmc" then
      tHudIcons = {
        Pmc = "HUD_objective_action"
      }
      icon_r, icon_g, icon_b = MrxUtil.GetPrimaryObjectiveRgb()
      disc_r, disc_g, disc_b = MrxUtil.GetPrimaryObjectiveRgb()
    elseif bStarterIsBoss then
      if bUnlocked then
        tHudIcons = {
          All = "HUD_HQ_AN",
          Chi = "HUD_HQ_CH",
          Gur = "HUD_HQ_GR",
          Oil = "HUD_HQ_OC"
        }
      else
        tHudIcons = {
          All = "HUD_HQ_AN_locked",
          Chi = "HUD_HQ_CH_locked",
          Gur = "HUD_HQ_GR_locked",
          Oil = "HUD_HQ_OC_locked"
        }
      end
      icon_r, icon_g, icon_b = 255, 255, 255
      disc_r, disc_g, disc_b = MrxUtil.GetPrimaryObjectiveRgb()
    else
      if bUnlocked then
        tHudIcons = {
          All = "HUD_Outpost_AN",
          Chi = "HUD_Outpost_CH",
          Gur = "HUD_Outpost_GR",
          Oil = "HUD_Outpost_OC",
          Pir = "HUD_Outpost_PR"
        }
      else
        tHudIcons = {
          All = "HUD_Outpost_AN_locked",
          Chi = "HUD_Outpost_CH_locked",
          Gur = "HUD_Outpost_GR_locked",
          Oil = "HUD_Outpost_OC_locked",
          Pir = "HUD_Outpost_PR_locked"
        }
      end
      icon_r, icon_g, icon_b = 255, 255, 255
      disc_r, disc_g, disc_b = MrxUtil.GetSecondaryObjectiveRgb()
    end
    local sIcon = tHudIcons[sFactionId]
    if self.sWorldIcon then
      sIcon = self.sWorldIcon
    end
    self._uMarker = Marker.AddBlip(uGuid, sIcon, 32, icon_r, icon_g, icon_b, 255, 1.25, 5, 175)
    if Net.IsServer() then
      Net.SendEvent_AddMarkerObjective(uGuid, self._uMarker, icon_r, icon_g, icon_b, 1.25, MrxUtil.MarkerGetIndexByName_World(sIcon or ""), 1, 16, false, 5, 175)
    end
    if not self._uDisc then
      self._uDisc = Marker.AddDisc(uGuid, 0.5, disc_r, disc_g, disc_b, 0.025)
      if Net.IsServer() then
        Net.SendEvent_AddMarkerObjective(uGuid, self._uDisc, disc_r, disc_g, disc_b, 0.025, 0, 0.5, 0, true)
      end
    end
  else
    Pg.RemoveContextAction(uGuid)
    if self._uEvent then
      Event.Delete(self._uEvent)
      self._uEvent = nil
    end
    HideTutorialMessage(self)
    if self._uMarker then
      Marker.Remove(self._uMarker)
      if Net.IsServer() then
        Net.SendEvent_RemoveMarkerObjective(self._uMarker)
      end
      self._uMarker = nil
    end
    if self._uDisc then
      Marker.Remove(self._uDisc)
      if Net.IsServer() then
        Net.SendEvent_RemoveMarkerObjective(self._uDisc)
      end
      self._uDisc = nil
    end
  end
end

function AddStarter(self, tStarter)
  if self._tStarter then
    Debug.Printf("Failed to add " .. tStarter:GetName() .. " - " .. self._tStarter:GetName() .. " already assigned to HQ portal")
    return
  end
  Debug.Printf("Adding " .. tStarter:GetName() .. " to HQ")
  self._tStarter = tStarter
  local nLandingZone = self.nLandingZone
  local sLzUnlockStyle = self.sLzUnlockStyle
  if nLandingZone and sLzUnlockStyle == "auto" then
    MrxTransit.SetLocationEnabled(nLandingZone, self:GetFaction())
    self:RefreshUiDisplay()
  end
end

function RemoveStarter(self, tStarter)
  if self._tStarter ~= tStarter then
    Debug.Printf("Failed to remove " .. tStarter:GetName() .. " - " .. tStarter:GetName() .. " is not assigned to HQ portal")
    return
  end
  Debug.Printf("Removing " .. tStarter:GetName())
  self._tStarter = nil
  self:RefreshUiDisplay()
end

function GetStarter(self)
  return self._tStarter
end

function GlobalEnter(bPmc)
  MrxFactionManager.DisableReporting(true)
  WifVzBoundary.SetInteriorMode(true)
  _ToggleHuds(false)
  MrxUtil.EnableHeroWeapons(false)
  MrxSound.EnterInterior()
  MrxLayerManager.ProcessMarkedLayers()
  local tPlayers = Player.GetAllPlayers()
  for _, uPlayerGuid in ipairs(tPlayers) do
    local uCharGuid = Player.GetCharacter(uPlayerGuid)
    Object.SetInvincible(uCharGuid, true, "HQ")
  end
end

function GlobalExit()
  MrxFactionManager.DisableReporting(false)
  WifVzBoundary.SetInteriorMode(false)
  _ToggleHuds(true)
  MrxUtil.EnableHeroWeapons(true)
  MrxSound.ExitInterior()
  Graphics.Camera.RestoreNearFar(0)
  local tPlayers = Player.GetAllPlayers()
  for _, uPlayerGuid in ipairs(tPlayers) do
    local uCharGuid = Player.GetCharacter(uPlayerGuid)
    Object.SetInvincible(uCharGuid, false, "HQ")
  end
end

function _OnEnter(self)
  if self._bInside then
    return
  end
  MrxFactionManager.DisableReporting(true)
  self._bInside = true
  if Net.IsServer() then
    Net.SetLoadingScreen(true)
    Net.SetBriefingInterior("MrxHq")
  end
  MrxState.Enter(MrxState.STATE_WAITFORGAME, _CompleteOnEnter, {self})
end

function _LockedOnEnter(self)
  if self._bInside then
    return
  end
  if self.sLockStatusMessage then
    local sLockedMessage
    if self.sLockStatusMessage == "Friendly" then
      sLockedMessage = "[Generic.HQ.Friendly]"
    elseif self.sLockStatusMessage == "Neutral" then
      sLockedMessage = "[Generic.HQ.Neutral]"
    elseif self.sLockStatusMessage == "NoEntry" then
      sLockedMessage = "[Generic.HQ.NoEntry]"
    elseif self.sLockStatusMessage == "NoContract" then
      sLockedMessage = "[Generic.HQ.NoContract]"
    end
    MrxTutorialManager.ShowMessage(sLockedMessage, true)
  end
  if not self._uHideMessage then
    self._uHideMessage = Event.Create(Event.TimerRelative, {5}, HideTutorialMessage, {self})
  end
end

function HideTutorialMessage(self)
  if self._uHideMessage then
    Event.Delete(self._uHideMessage)
    self._uHideMessage = nil
  end
  MrxTutorialManager.HideMessage(true)
end

function _CompleteOnEnter(self)
  GlobalEnter(false)
  self:RefreshUiDisplay()
  self._bInside = true
  MrxHqManager.SetInside(true)
  Debug.Printf("@@@@@@@@@@ MrxHq._OnEnter: refreshing UI display...")
  self:RefreshUiDisplay()
  WifFreePlay.StopNag()
  if Sound.StopAndFlushAllSounds then
    Sound.StopAndFlushAllSounds()
  end
  self:_LoadInterior()
  dynamic_import("MrxBriefing", self._BriefingModuleLoaded, {self})
  Graphics.Camera.SetNearFar(0, 0.3, self:GetDrawDistance(), 0)
end

function _LoadInterior(self)
  if self.tLayers then
    Debug.Printf("Loading layers for HQ")
    MrxLayerManager.Add(self.tLayers, nil, nil, nil, nil, true)
  end
  Debug.Printf("Spawning interior for HQ")
  local vPosition = {
    3750,
    450,
    -3840
  }
  local uGuid = MrxUtil.SpawnActor(self.tInterior.sTemplate, "HqInterior", vPosition, self.tInterior.sAnchorHardpoint, self.nRotation, false, false, self._OnInteriorLoad, {self})
end

function _OnInteriorLoad(self)
  Debug.Printf("Interior loaded")
  self._tStarter:Load(_KickoffStarter, {self, 1})
  MrxUtil.EnableHeroWeapons(false)
  Graphics.Atmosphere.ChangeLineRegionSetting(Pg.GetGuidByName("rgn_atmo_interior"), self:GetAtmosphere())
  MrxUtil.TeleportHeroesToHardpoints({
    {
      vObject = Pg.GetGuidByName("HqInterior"),
      sHardpoint = "hp_playerA_enter"
    }
  }, _KickoffStarter, {self, 2})
end

function _BriefingModuleLoaded(self, mModule)
  _mBriefingModule = mModule
  local oStarter = self:GetStarter()
  ASSERT(oStarter)
  oStarter:SetBriefingModule(_mBriefingModule)
  _mBriefingModule.LoadTableOfAssets(_tAssetPreload, _KickoffStarter, {self, 3})
end

function _KickoffStarter(self, nSignal)
  if nSignal == 1 then
    _bStarterLoaded = true
    Debug.Printf("@@@@@@@@@@ MrxHq._KickoffStarter: starter loaded")
  elseif nSignal == 2 then
    _bHeroTeleportComplete = true
    Debug.Printf("@@@@@@@@@@ MrxHq._KickoffStarter: teleport complete")
  elseif nSignal == 3 then
    _bGenericAssetsLoaded = true
    Debug.Printf("@@@@@@@@@@ MrxHq._KickoffStarter: generic assets loaded")
  end
  if not (_bStarterLoaded and _bHeroTeleportComplete) or not _bGenericAssetsLoaded then
    return
  end
  Event.Create(Event.TimerRelative, {1}, function()
    _bStarterLoaded = nil
    _bHeroTeleportComplete = nil
    _bGenericAssetsLoaded = nil
    self._tStarter:Start()
  end)
end

function ExitBegin(self)
  if not self._bInside then
    return
  end
  Debug.Printf("Exiting Hq")
  GlobalExit()
  self:Unload()
  local oStarter = self:GetStarter()
  if oStarter then
    oStarter:CardDisplayed()
  end
  self._bInside = false
  MrxHqManager.SetInside(false)
  _mBriefingModule.UnloadTableOfAssets(_tAssetPreload)
  if Net.IsServer() then
    Net.SetBriefingInterior()
  end
  _mBriefingModule = nil
  dynamic_remove("MrxBriefing")
end

function ExitEnd(self)
  Event.Create(Event.TimerRelative, {2}, function()
    local nLandingZone = self.nLandingZone
    local sLzUnlockStyle = self.sLzUnlockStyle
    if nLandingZone and sLzUnlockStyle == "visit" then
      MrxTransit.SetLocationEnabled(nLandingZone, self:GetFaction())
    end
    local tMissionsToBeAccepted, sLastAcceptedMission = self._tStarter:GetMissionsToBeAccepted()
    if type(tMissionsToBeAccepted) == "table" and table.getn(tMissionsToBeAccepted) > 0 then
      WifMissionFlow.AcceptMissions(tMissionsToBeAccepted, sLastAcceptedMission)
      local uParkingLotPoint = Pg.GetGuidByName(self.sParkingLot)
      local uHeliPoint
      if self.nLandingZone then
        uHeliPoint = MrxTransit.GetTransitPoint(self.nLandingZone)
      elseif self.nAltLandingZone then
        uHeliPoint = MrxTransit.GetTransitPoint(self.nAltLandingZone)
      else
        uHeliPoint = uParkingLotPoint
      end
      Event.Post("parkingLotStart", {
        Pg.GetGuidByName(self.tPortal.sEntrance),
        uParkingLotPoint,
        uHeliPoint
      })
    end
    self._tStarter:ResetIntraSessionData()
    MrxState.Exit(MrxState.STATE_WAITFORGAME)
    local fCallback, tCallbackArgs = MrxHqManager.GetUnloadCallback()
    MrxUtil.CallWithOptionalArgs(fCallback, tCallbackArgs)
    MrxHqManager.SetUnloadCallback(nil, nil)
    Debug.Printf("@@@@@@@@@@ MrxHq.ExitBegin: refreshing UI display...")
    self:RefreshUiDisplay()
    WifFreePlay.StartNag()
  end)
end

function _ToggleHuds(bEnable)
  local tPlayers = Player.GetAllPlayers()
  for _, uPlayer in ipairs(tPlayers) do
    MrxGuiBootstrap.ToggleHud(uPlayer, bEnable, "briefing")
  end
end

function Unload(self)
  if self.tLayers then
    Debug.Printf("Unloading layers for HQ")
    MrxLayerManager.Remove(self.tLayers, nil, nil, true)
  end
  if self.tInterior then
    Debug.Printf("Removing interior from HQ")
    local uGuid = Pg.GetGuidByName("HqInterior")
    Object.Remove(uGuid)
  end
  if self._tStarter then
    self._tStarter:Unload()
  end
end
