import("MrxGui")
import("MrxTask")
import("MrxUtil")
import("MrxLayerManager")
import("MrxHqManager")
import("MrxPlayState")
import("MrxState")
import("WifPmcInterior")
import("WifBriefingData")
import("WifMissionData")
import("MrxUnlockFanfare")
import("WifMissionFlow")
import("MrxSoundBanks")
import("MrxTransit")

function Create(mModule, self)
  self = self or {}
  setmetatable(self, {__index = mModule})
  self._tBriefings = {}
  self:_SetBriefingCount(0)
  return self
end

function GetName(self)
  return self.sName
end

function GetPmcName(self)
  local sStarterName = self:GetName()
  if sStarterName == "PmcBoss" then
    return "Fiona"
  elseif sStarterName == "HelPmcBoss" then
    return "Ewan"
  elseif sStarterName == "JetPmcBoss" then
    return "Misha"
  elseif sStarterName == "MecPmcBoss" then
    return "Eva"
  end
  return sStarterName
end

function GetActionDisplay(self)
  return self.sActionDisplay
end

function GetHq(self)
  return self.sHqName
end

function IsBoss(self)
  return self.bBoss
end

function IsPmcStarter(self)
  return self.bPmcStarter
end

function IsMale(self)
  return not self.bFemale
end

function HasHintSystem(self)
  return self.bHintSystem
end

function HasBribeSystem(self)
  return self.bBribeSystem
end

function HasGarageSystem(self)
  return self.bGarageSystem
end

function HasTransitSystem(self)
  return self.bTransitSystem
end

function GetFaction(self)
  return self.sFaction
end

function GetCardData(self)
  return self.tCardData
end

function GetPlayerVisibleName(self)
  return self.sPlayerVisibleName
end

function HasShop(self)
  return self.bShop or self.bCustomVehicleShop
end

function HasCustomVehicleShop(self)
  return self.bCustomVehicleShop
end

function SetActor(self, uGuid)
  self._uGuid = uGuid
end

function GetActor(self)
  return self._uGuid
end

function _SetFanfareDisplayed(self, bDisplayed)
  self._bFanfareDisplayed = bDisplayed
end

function FanfareDisplayed(self)
  self:_SetFanfareDisplayed(true)
end

function HasFanfareBeenDisplayed(self)
  return MrxUtil.SetDefault(self._bFanfareDisplayed, false)
end

function _SetCardDisplayed(self, bDisplayed)
  self._bCardDisplayed = bDisplayed
  if self:IsActivated() then
    self:RefreshBriefingRoomDisplay()
  end
end

function CardDisplayed(self)
  self:_SetCardDisplayed(true)
end

function HasCardBeenDisplayed(self)
  return MrxUtil.SetDefault(self._bCardDisplayed, false)
end

function GetGlobalFaceFxSet(self)
  return self.sFaceFxSet
end

function AddBriefing(self, sMissionName, sMissionTitle)
  if self._tBriefings[sMissionName] then
    return
  end
  local sMissionLevel
  if WifMissionData.GetMissionRepeatable(sMissionName) then
    local nCompleted = WifMissionFlow.GetKeyValue(sMissionName)
    nCompleted = nCompleted + 1
    if nCompleted > WifMissionData.GetMissionLevels(sMissionName) then
      nCompleted = WifMissionData.GetMissionLevels(sMissionName)
    end
    sMissionLevel = "([Generic.Level] " .. nCompleted .. ")"
  end
  self._tBriefings[sMissionName] = {sTitle = sMissionTitle, sLevel = sMissionLevel}
  local nCount = self:_GetBriefingCount()
  self:_SetBriefingCount(nCount + 1)
  if not self:IsActivated() then
    self:Activate()
  else
    self:RefreshBriefingRoomDisplay()
  end
end

function RemoveBriefing(self, sMissionName)
  if not self._tBriefings[sMissionName] then
    return
  end
  Debug.Printf("Starter " .. self:GetName() .. " removing briefing " .. sMissionName)
  self._tBriefings[sMissionName] = nil
  local nCount = self:_GetBriefingCount()
  self:_SetBriefingCount(nCount - 1)
  self:RefreshBriefingRoomDisplay()
end

function GetOfferedBriefings(self)
  return self._tBriefings
end

function _SetBriefingCount(self, nBriefingCount)
  ASSERT(0 <= nBriefingCount)
  self._nBriefingCount = nBriefingCount
end

function _GetBriefingCount(self)
  return self._nBriefingCount
end

function SetBriefingOld(self, sMissionName)
  if not self._tOldBriefings then
    self._tOldBriefings = {}
  end
  self._tOldBriefings[sMissionName] = true
end

function IsBriefingOld(self, sMissionName)
  if not self._tOldBriefings then
    return false
  end
  return self._tOldBriefings[sMissionName] == true
end

function GetOldBriefings(self)
  return self._tOldBriefings
end

function AddIntro(self, sName)
  if self._tIntros and self._tIntros[sName] ~= nil then
    return
  end
  if not self._tIntros then
    self._tIntros = {}
  end
  self._tIntros[sName] = false
  if self:IsActivated() then
    self:RefreshBriefingRoomDisplay()
  end
end

function RemoveIntro(self, sName)
  if not self._tIntros then
    return
  end
  self._tIntros[sName] = nil
  if self:IsActivated() then
    self:RefreshBriefingRoomDisplay()
  end
end

function SetViewedIntro(self, sName, bViewed, bRefresh)
  if not self._tIntros or self._tIntros[sName] == nil then
    return false
  end
  self._tIntros[sName] = bViewed
  if bViewed and not WifMissionFlow.HasKey(sName .. "Intro") then
    WifMissionFlow.AwardKey(sName .. "Intro")
    if bRefresh then
      WifMissionFlow.EnableAutosave()
      WifMissionFlow.Refresh()
    end
  end
  if self:IsActivated() then
    self:RefreshBriefingRoomDisplay()
  end
end

function HasViewedIntro(self, sName)
  if not self._tIntros then
    return false
  end
  return self._tIntros[sName]
end

function GetIntros(self)
  return self._tIntros
end

function HasIntros(self)
  return self._tIntros ~= nil
end

function SetMissionAccepted(self, sMissionName, bAccepted)
  if not self._tBriefings[sMissionName] then
    return
  end
  self._tBriefings[sMissionName].bAccepted = bAccepted
  self:SetBriefingOld(sMissionName)
end

function IsMissionAccepted(self, sMissionName)
  if not self._tBriefings[sMissionName] then
    return false
  end
  return self._tBriefings[sMissionName].bAccepted
end

function GetMissionsToBeAccepted(self)
  return self._tMissionsToBeAccepted, self._sLastAcceptedMission
end

function SetPendingContract(self, sPendingContractId)
  self._sPendingContractId = sPendingContractId
end

function GetPendingContract(self)
  return self._sPendingContractId
end

function IsContractPending(self)
  return self._sPendingContractId ~= nil
end

function ResetIntraSessionData(self)
  self._tMissionsToBeAccepted = nil
  self._sLastAcceptedMission = nil
  self._sPendingContractId = nil
end

function HasCriticalPathBriefings(self)
  for sMissionName, _ in pairs(self._tBriefings) do
    if WifMissionData.IsMissionOnCriticalPath(sMissionName) and not WifMissionFlow.HasKey(sMissionName) then
      return true
    end
  end
  return false
end

function Activate(self)
  Debug.Printf("Starter activated!")
  local sHqName = self:GetHq()
  if sHqName then
    MrxHqManager.AddStarter(sHqName, self)
  end
  if not self:HasFanfareBeenDisplayed() then
    self:FanfareDisplayed()
    if self:IsBoss() and self:GetFaction() ~= "Pmc" then
    elseif self:GetCardData() then
    end
  end
  self._bActive = true
  self:RefreshBriefingRoomDisplay()
end

function Deactivate(self)
  local sHqName = self:GetHq()
  if sHqName then
    MrxHqManager.RemoveStarter(sHqName, self)
  end
  self:Unload()
  self._bActive = nil
end

function IsActivated(self)
  return self._bActive
end

function Start(self)
  local mModule = self._mBriefingModule
  mModule.SetStarter(self)
  mModule.SetBriefingWrapper(self.tBriefingWrapper)
  mModule.Start()
end

function GetBriefingWrapper(self)
  return self.tBriefingWrapper
end

function End(self, tMissionsAcceptedThisSession, sLastAcceptedMission)
  self._tMissionsToBeAccepted = tMissionsAcceptedThisSession
  self._sLastAcceptedMission = sLastAcceptedMission
  local sHqName = self:GetHq()
  local bPmcStarter = self:IsPmcStarter()
  if sHqName then
    local oHq = MrxHqManager.GetHq(sHqName)
    if oHq then
      self:_CompleteHqExit(oHq)
    end
  elseif bPmcStarter then
    local bResetStarter = sLastAcceptedMission ~= nil
    WifPmcInterior.BriefingComplete(bResetStarter)
    if sLastAcceptedMission then
      WifPmcInterior.Exit(1, false)
    elseif not MrxTransit.IsInTransit() then
      MrxState.Exit(MrxState.STATE_WAITFORGAME)
    end
  end
end

function _CompleteHqExit(self, oHq)
  oHq:ExitBegin()
  local sPendingContract = self:GetPendingContract()
  local tExitTo
  if sPendingContract then
    tExitTo = WifMissionFlow.GetMissionStartLocations(sPendingContract)
  else
    tExitTo = oHq:GetEntryLocations()
  end
  MrxUtil.TeleportHeroesToLocations(tExitTo, oHq.ExitEnd, {oHq})
end

function Load(self, fCallback, tCallbackData)
  if self._bLoaded then
    MrxUtil.CallWithOptionalArgs(fCallback, tCallbackData)
    return
  end
  Debug.Printf("Starter loading")
  
  local function _Loaded()
    local uGuid = self:GetActor()
    uGuid = uGuid or Pg.GetGuidByName("Starter")
    if uGuid then
      self:SetActor(uGuid)
      if self.sFaceFxSet then
        local bSuccess = Animation.BindFaceAnimSet(uGuid, self.sFaceFxSet)
      end
    end
    MrxUtil.CallWithOptionalArgs(fCallback, tCallbackData)
  end
  
  MrxUtil.SetupLoadingCallback(self, _Loaded)
  local tData = {self}
  _tAssetLoadTimers = _tAssetLoadTimers or {}
  
  local function _AssetLoaded(sName, sAssetType, bTimerTriggered)
    local uTimer = _tAssetLoadTimers[sAssetType][sName]
    if uTimer then
      Event.Delete(uTimer)
      _tAssetLoadTimers[sAssetType][sName] = nil
      if bTimerTriggered then
        Debug.Printf("@@@@@@@@@@ MrxStarter._AssetLoaded: asset " .. sName .. " (" .. sAssetType .. ") TIMED OUT")
      else
        Debug.Printf("@@@@@@@@@@ MrxStarter._AssetLoaded: loaded " .. sName .. " (" .. sAssetType .. ")")
      end
      MrxUtil.CallWithOptionalArgs(MrxUtil.LoadingCallback, tData)
    else
      Debug.Printf("@@@@@@@@@@ MrxStarter._AssetLoaded: timed out or already loaded " .. sName .. " (" .. sAssetType .. ")")
    end
  end
  
  if not Net.IsClient() then
    if self.tLayers then
      self._nLoadPending = self._nLoadPending + 1
      MrxLayerManager.Add(self.tLayers, MrxUtil.LoadingCallback, tData, true, true)
    end
    if self:IsBoss() then
      local tBriefings = self:GetOfferedBriefings()
      local tMissionIds = {}
      for sMissionName, tMissionData in pairs(tBriefings) do
        table.insert(tMissionIds, sMissionName)
      end
      local sMissionId = tMissionIds[1]
      local tBriefingConfig = WifBriefingData[sMissionId]
      if tBriefingConfig and tBriefingConfig.tActors then
        self.tActors = self.tActors or {}
        for sName, tActorData in pairs(tBriefingConfig.tActors) do
          self.tActors[sName] = tActorData
        end
      end
    end
    if self.tActors then
      self._tQualityRefs = {}
      for sName, tActorData in pairs(self.tActors) do
        Debug.Printf("Unlock key: ", tActorData.sUnlockKey, WifMissionFlow.HasKey(tostring(tActorData.sUnlockKey)))
        if not tActorData.sUnlockKey or WifMissionFlow.HasKey(tActorData.sUnlockKey) then
          self._nLoadPending = self._nLoadPending + 1
          local uGuid = MrxUtil.SpawnActor(tActorData.sTemplate, sName, "HqInterior", tActorData.sPosition, nil, false, true, MrxUtil.LoadingCallback, tData)
          self._tQualityRefs[uGuid] = Object.AddQualityRef(uGuid, 1)
        end
      end
    end
  end
  if self.tAssetPreload then
    for sAssetType, tAssets in pairs(self.tAssetPreload) do
      for _, sAssetName in ipairs(tAssets) do
        self._nLoadPending = self._nLoadPending + 1
        if sAssetType == "soundbank" or sAssetType == "wavebank" then
          MrxSoundBanks.LoadTempBank(sAssetName, sAssetType, _AssetLoaded, {
            sAssetName,
            sAssetType,
            false
          })
        else
          Pg.LoadAsset(sAssetName, sAssetType, _AssetLoaded, {
            sAssetName,
            sAssetType,
            false
          })
        end
        _tAssetLoadTimers[sAssetType] = _tAssetLoadTimers[sAssetType] or {}
        _tAssetLoadTimers[sAssetType][sAssetName] = Event.Create(Event.TimerRelative, {15, false}, _AssetLoaded, {
          sAssetName,
          sAssetType,
          true
        })
        Debug.Printf("@@@@@@@@@@ MrxStarter.Load: Loading asset " .. sAssetName .. " (type: " .. sAssetType .. ")")
      end
    end
  end
  if self.sFaceFxSet then
    self._nLoadPending = self._nLoadPending + 1
    Pg.LoadAsset(self.sFaceFxSet, "facefxanimationset", MrxUtil.LoadingCallback, tData)
  end
  self._bLoaded = true
  if self._nLoadPending == 0 then
    Debug.Printf("Nothing to load...")
    MrxUtil.CleanupLoadingCallback(self)
    MrxUtil.CallWithOptionalArgs(fCallback, tCallbackData)
  end
end

function Unload(self)
  if not self._bLoaded then
    return
  end
  Debug.Printf("Starter Unloading\n" .. tostring(Debug.GetCallstack()))
  if not Net.IsClient() then
    if self.tLayers then
      MrxLayerManager.Remove(self.tLayers, nil, nil, true)
    end
    if self.tActors then
      for sName, tActorData in pairs(self.tActors) do
        local uGuid = Pg.GetGuidByName(sName)
        if uGuid then
          Debug.Printf("Removing " .. sName)
          Object.Remove(uGuid)
        end
      end
      if self._tQualityRefs then
        for uGuid, uRef in pairs(self._tQualityRefs) do
          Object.RemoveQualityRef(uRef)
        end
        self._tQualityRefs = nil
      end
    end
  end
  if self.tAssetPreload then
    for sAssetType, tAssets in pairs(self.tAssetPreload) do
      for _, sAssetName in ipairs(tAssets) do
        if sAssetType == "soundbank" or sAssetType == "wavebank" then
          MrxSoundBanks.UnloadTempBank(sAssetName, sAssetType)
        else
          Pg.UnloadAsset(sAssetName, sAssetType)
        end
        Debug.Printf("@ Unloaded asset " .. sAssetName .. "." .. sAssetType)
      end
    end
  end
  if self.sFaceFxSet then
    Pg.UnloadAsset(self.sFaceFxSet, "facefxanimationset")
    Debug.Printf("@ Unloaded asset " .. self.sFaceFxSet .. ".facefxanimationset")
    local uGuid = Pg.GetGuidByName("Starter")
    if uGuid then
      local bSuccess = Animation.UnbindFaceAnimSet(uGuid, self.sFaceFxSet)
    end
  end
  self:SetActor(nil)
  self._bLoaded = nil
end

function RefreshBriefingRoomDisplay(self)
  local sHqName = self:GetHq()
  local bPmcStarter = self:IsPmcStarter()
  if sHqName then
    local oHq = MrxHqManager.GetHq(sHqName)
    if oHq then
      oHq:RefreshUiDisplay()
    end
  elseif bPmcStarter then
    WifPmcInterior.RefreshUiDisplay()
  end
end

function SetBriefingModule(self, mModule)
  self._mBriefingModule = mModule
end

function SetSpecialCaseGreeting(self, sVo)
  self._sSpecialCaseGreeting = sVo
end

function GetSpecialCaseGreeting(self)
  return self._sSpecialCaseGreeting
end
