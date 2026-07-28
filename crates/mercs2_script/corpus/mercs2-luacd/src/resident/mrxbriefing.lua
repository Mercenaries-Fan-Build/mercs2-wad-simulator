import("MrxCinematic")
import("WifEquipmentData")
import("MrxFactionManager")
import("MrxGui")
import("MrxGuiDialogBox")
import("MrxUtil")
import("MrxVoSequence")
import("WifBriefingData")
import("WifHints")
import("WifMissionData")
import("WifMissionFlow")
import("WifPmcInterior")
import("WifRecommendationData")
import("MrxPlayState")
import("MrxState")
import("MrxTransit")
import("MrxSupportTransit")
import("MrxStarterManager")
import("WifHqData")
import("MrxRewardData")
import("MrxPmc")
import("MrxSoundBanks")
import("WifStarterData")
import("MrxShop")
CHEAP_GREETING = 1
CHEAP_SPECIALCASEGREETING = 2
CHEAP_STARTINTRO = 3
CHEAP_JOBREQUEST = 4
CHEAP_JOBACCEPT = 5
CHEAP_JOBDECLINE = 6
CHEAP_WAGERBEGINWIN = 7
CHEAP_WAGERBEGINLOSE = 8
CHEAP_WAGERWON = 9
CHEAP_WAGERLOST = 10
CHEAP_WAGERCHICKENSUIT = 11
CHEAP_HINT = 12
CHEAP_GOODBYE = 13
CHEAP_CONFIRM = 14
CHEAP_DECLINE = 15
CHEAP_INTRO = 16
CHEAP_PMCWAGER = 17
_tHeroWrapperVo = {
  tJobRequest = {
    Chris = {
      "Chris.Greeting01",
      "Chris.Greeting02",
      "Chris.Greeting03"
    },
    Mattias = {
      "Mattias.Greeting01",
      "Mattias.Greeting02",
      "Mattias.Greeting03"
    },
    Jennifer = {
      "Jen.Greeting01",
      "Jen.Greeting02",
      "Jen.Greeting03"
    }
  },
  tJobAccept = {
    Chris = {
      "Chris.Yes01",
      "Chris.Yes02",
      "Chris.Yes03"
    },
    Mattias = {
      "Mattias.Yes01",
      "Mattias.Yes02",
      "Mattias.Yes03"
    },
    Jennifer = {
      "Jen.Yes01",
      "Jen.Yes02",
      "Jen.Yes03"
    }
  },
  tJobDecline = {
    Chris = {
      "Chris.No01",
      "Chris.No02",
      "Chris.No03"
    },
    Mattias = {
      "Mattias.No01",
      "Mattias.No02",
      "Mattias.No03"
    },
    Jennifer = {
      "Jen.No01",
      "Jen.No02",
      "Jen.No03"
    }
  }
}
_tViewedIntros = {}
_tDefaultCameraEffects = {
  DOF = {
    nAngle = 0,
    nStartNear = 0,
    nEndNear = 0.3,
    nStartFar = 4,
    nEndFar = 10,
    nBlur = 0.5
  },
  FOV = {nAngle = 55}
}
_ClientMenuBox = nil
_ClientJoinEvent = nil
_ClientMenuPending = nil

function Deinit()
  _CheckAssets()
end

function SetStarter(oStarter)
  _oStarter = oStarter
  if not _ClientJoinEvent then
    _ClientJoinEvent = Event.CreatePersistent(Event.ScriptEvent, {
      "mpPlayerJoin",
      function(tData)
        return Net.IsServer() and not Player.IsLocal(tData[1])
      end
    }, SendPlayerJoinEvents)
  end
  if Net.IsServer() then
    local sStarterId = _oStarter:GetName()
    local tNetUnlockedRewards = MrxShop.GetIndexedShopList(oStarter)
    local idx = 1
    local tNetStarterActors = {}
    tNetStarterActors[idx] = GetActorGuid("HqInterior")
    idx = idx + 1
    tNetStarterActors[idx] = GetActorGuid("Starter")
    idx = idx + 1
    if sStarterId then
      local tActors = WifStarterData[sStarterId].tActors
      if tActors then
        for sName, tActorData in pairs(tActors) do
          if sName ~= "Starter" then
            tNetStarterActors[idx] = GetActorGuid(sName)
            Debug.Printf("WifStarterData[ " .. tostring(sStarterId) .. " ].tActor[ " .. tostring(sName) .. " ] = " .. tostring(tNetStarterActors[idx]))
            idx = idx + 1
          end
        end
      end
    end
    Net.SetBriefingStarters(MrxStarterManager.GetStarterIndexFromName(sStarterId), tNetStarterActors, tNetUnlockedRewards)
  end
end

function SetBriefingWrapper(tBriefingWrapper)
  _tBriefingWrapper = tBriefingWrapper
end

function SendPlayerJoinEvents()
  if not Net.IsServer() then
    return
  end
  if _ClientMenuBox then
    Net.SendCustomEvent("MrxBriefing", NETEVENT_DISPLAYMENU, {})
  end
end

function NetSafeBriefingAssetsLoaded()
  _bLoadingBriefingAssets = nil
  _bBriefingAssetsLoaded = true
  Debug.Printf("_bBriefingAssetsLoaded = true")
  MrxState.Exit(MrxState.STATE_WAITFORGAME)
  if _tForceUnloadBriefingAssets then
    NetSafeUnloadBriefingAssets(_tForceUnloadBriefingAssets)
    _tForceUnloadBriefingAssets = nil
  end
end

function NetSafeAreBriefingAssetsLoaded()
  return _bBriefingAssetsLoaded
end

function NetSafeLoadBriefingAssets(tAssetTable)
  if not _bLoadingBriefingAssets then
    Debug.Printf("_bBriefingAssetsLoaded = nil")
    _bBriefingAssetsLoaded = nil
    MrxState.Enter(MrxState.STATE_WAITFORGAME, LoadTableOfAssets, {tAssetTable, NetSafeBriefingAssetsLoaded})
  end
  _bLoadingBriefingAssets = true
end

function NetSafeUnloadBriefingAssets(tAssetTable)
  if _bLoadingBriefingAssets then
    _tForceUnloadBriefingAssets = tAssetTable
  else
    _bLoadingBriefingAssets = nil
    _bBriefingAssetsLoaded = nil
    Debug.Printf("_bBriefingAssetsLoaded = nil")
    UnloadTableOfAssets(tAssetTable)
  end
end

function NetSafeShowFlashBriefing(uName)
  local sFlashName
  for sName, oFlash in pairs(_tFlashObjects) do
    if String.GetHash(sName) == uName then
      sFlashName = sName
    end
  end
  if sFlashName then
    _ShowFlashObject(sFlashName)
  end
end

function NetSafeRemoveFlashBriefing(uName)
  local sFlashName
  for sName, oFlash in pairs(_tFlashObjects) do
    if String.GetHash(sName) == uName then
      sFlashName = sName
    end
  end
  if sFlashName then
    _RemoveFlashObject(sFlashName)
  end
end

function NetSafeIsStarterLoaded()
  return _starterLoaded
end

function NetSafeStarterLoaded()
  MrxState.SetQuickFade(true)
  _starterLoaded = true
end

function NetSafeSetStarter(starterIndex, tActors, tShopUnlocked)
  MrxState.SetQuickFade(false)
  _starterLoaded = nil
  Vehicle.Exit(tActors[1], tActors[2])
  MrxShop.SetIndexedShopList(tShopUnlocked)
  local sStarterName = MrxStarterManager.GetStarterNameFromIndex(starterIndex)
  _oStarter = MrxStarterManager.RequestStarter(sStarterName)
  _oStarter:SetActor(tActors[2])
  _oStarter:Load(NetSafeStarterLoaded)
  local idx = 3
  if not Object.GetName(tActors[1]) then
    Object.SetName(tActors[1], "HqInterior")
  end
  if sStarterName then
    local tStarterActors = WifStarterData[sStarterName].tActors
    if tStarterActors then
      for sName, tActorData in pairs(tStarterActors) do
        if sName ~= "Starter" then
          if tActors[idx] then
            Debug.Printf("Setting name of " .. tostring(tActors[idx]) .. " to " .. tostring(sName))
            Object.SetName(tActors[idx], sName)
          end
          idx = idx + 1
        end
      end
    end
  end
  SetBriefingWrapper(_oStarter:GetBriefingWrapper())
  _nBaseShadowDistance = Graphics.GetShadowBaseDistance()
  Graphics.SetShadowBaseDistance(2)
  if _oStarter:IsPmcStarter() then
    _SaveActorsOriginalPositions({"Player1", "Starter"})
    local sStarterId = _oStarter:GetName()
    local tBriefingLocs = WifPmcInterior.GetStarterBriefingLocs(sStarterId)
    _AttachActorsToLocations({
      Player1 = tBriefingLocs[2],
      Starter = tBriefingLocs[1]
    })
    Object.SetTransformToObject(Player.GetSecondaryCharacter(), Pg.GetGuidByName(tBriefingLocs[2]))
  else
    _AttachActorsToHardpoints({Player1 = "hp_playerA", Starter = "hp_starter"})
    Object.SetTransformToObject(Player.GetSecondaryCharacter(), GetActorGuid("HqInterior"), "hp_playerA")
  end
  _SetupPlayers(false)
  Gui.EnablePlayerMarkers(false)
  if not _oStarter:IsBoss() then
    Human.DisableWeapons(GetActorGuid("Starter"))
  end
  VO.Cancel()
  VO.SetCinematicMode(true)
  _BindFaceAnim(true, "Player1")
  _BindFaceAnim(true, "Starter")
  _SetActorsToDefaultPose({"Player1", "Starter"}, false)
  if not _oStarter:IsBoss() and _oStarter:IsPmcStarter() then
    _ProcessCameraSettings({
      tShot = {
        sName = "OverTheShoulderLeft",
        sBaseActor = "Starter",
        sTargetActor = "Player1"
      }
    })
  end
  local sHqName = _oStarter:GetHq()
  if sHqName then
    local tHq = WifHqData.GetHqConfigFromId(sHqName)
    if tHq then
      Graphics.Atmosphere.ChangeLineRegionSetting(Pg.GetGuidByName("rgn_atmo_interior"), tHq.sAtmosphere)
    end
  end
  if _ClientMenuPending then
    _DisplayClientMenu()
  end
end

function NetSafeClearStarter()
  MrxState.SetQuickFade(false)
  _starterLoaded = nil
  if _oStarter then
    local uGuid = _oStarter:GetActor()
    _oStarter:Unload()
    _oStarter:SetActor(uGuid)
  end
  MrxShop.SetIndexedShopList(nil)
  _ClearAllFlashObjects()
  _End()
end

function NetSafeLoadSpiel(nMissionIndex, tActors)
  MrxState.SetQuickFade(true)
  _Fade(false)
  if _ClientWaitBox then
    _ClientWaitBox:Close()
    _ClientWaitBox = nil
  end
  _bNetSafeSpielLoaded = nil
  local sMissionName = WifMissionData.GetMissionIdFromIndex(nMissionIndex)
  _sSelectedMission = sMissionName
  if not _tBriefings then
    _tBriefings = {}
  end
  _tBriefings[_sSelectedMission] = {}
  _tBriefings[_sSelectedMission].tConfig = WifBriefingData[sMissionName] or {}
  local idx = 1
  local tBriefingActors = _tBriefings[_sSelectedMission].tConfig.tActors
  if tBriefingActors then
    for sName, tActorData in pairs(tBriefingActors) do
      if sName ~= "Starter" then
        if not Object.GetName(tActors[idx]) then
          Object.SetName(tActors[idx], sName)
        end
        idx = idx + 1
      end
    end
  end
  local sSpielFile = GetSpielFileName(sMissionName)
  local tConfig = WifBriefingData[sMissionName] or {}
  if tConfig and tConfig.tPositions then
    _AttachActorsToHardpoints(tConfig.tPositions)
  end
  dynamic_import(sSpielFile, _FileLoaded)
end

function NetSafeIsSpielLoaded()
  return _bNetSafeSpielLoaded
end

function NetSafeUnloadSpiel(nMissionIndex)
  _bNetSafeSpielLoaded = nil
  _ClearAllFlashObjects()
  _CinematicComplete()
  MrxState.SetQuickFade(false)
end

function NetSafePlayCheapCinematic(nCinematicType, nIntroIndex)
  _StopCheapCinematic()
  if nCinematicType == CHEAP_CONFIRM or nCinematicType == CHEAP_DECLINE then
    local tConfig = _GetSelectedBriefingConfig()
    if tConfig and _bNetSafeSpielLoaded then
      local tCinematic
      if nCinematicType == CHEAP_CONFIRM then
        tCinematic = tConfig.tConfirmCinematic
      elseif nCinematicType == CHEAP_DECLINE then
        tCinematic = tConfig.tDeclineCinematic
      end
      if tCinematic then
        local sCharName = MrxUtil.GetPrimaryCharacterName()
        if tCinematic.tCameraEffects then
          _ProcessCameraEffects(tConfig, tCinematic.tCameraEffects[sCharName])
        end
        _ProcessCameraSettings({
          sPositionObject = "Player1",
          sPositionHardpoint = "Bone_Attach_Root",
          sLookAtObject = "Player1",
          sLookAtHardpoint = "Bone_Attach_Root",
          bLookAtDirection = true
        })
        _NextCinematicFrame(tCinematic[sCharName])
      end
    end
  elseif nCinematicType == CHEAP_INTRO then
    _PlayIntro(WifBriefingData.GetIntroIdByIndex(nIntroIndex))
  else
    local tCheapCinematic = _CreateCheapCinematic(nCinematicType)
    Debug.Printf("NetSafePlayCheapCinematic( " .. tostring(nCinematicType) .. ")")
    Debug.Printf("tCheapCinematic = " .. tostring(tCheapCinematic))
    if tCheapCinematic then
      if not tCheapCinematic.tSequence then
        Debug.Printf("Couldn't create cheap cinematic on client")
        tCheapCinematic.tSequence = {}
      end
      _ProcessCheapCinematic(tCheapCinematic, _StopClientCheapCinematic)
    end
  end
end

function GetViewedIntros()
  return _tViewedIntros
end

function LoadTableOfAssets(tAssetTable, fCallback, tCallbackArgs)
  MrxUtil.SetupLoadingCallback(_THIS, fCallback, tCallbackArgs)
  _nLoadPending = _nLoadPending + 1
  for sAssetType, tAssets in pairs(tAssetTable) do
    _LoadTableOfAssets(sAssetType, tAssets, MrxUtil.LoadingCallback, {_THIS})
  end
  MrxUtil.LoadingCallback(_THIS)
end

function UnloadTableOfAssets(tAssetTable, fCallback, tCallbackArgs)
  MrxUtil.SetupLoadingCallback(_THIS, fCallback, tCallbackArgs)
  _nLoadPending = _nLoadPending + 1
  for sAssetType, tAssets in pairs(tAssetTable) do
    _UnloadTableOfAssets(sAssetType, tAssets, MrxUtil.LoadingCallback, {_THIS})
  end
  MrxUtil.LoadingCallback(_THIS)
end

function _LoadTableOfAssets(sAssetType, tAssets, fCallback, tCallbackData)
  _tAssetLoadTimers = _tAssetLoadTimers or {}
  
  local function _AssetLoaded(sName, bTimerTriggered)
    local uTimer = _tAssetLoadTimers[sAssetType][sName]
    if uTimer then
      Event.Delete(uTimer)
      _tAssetLoadTimers[sAssetType][sName] = nil
      if bTimerTriggered then
        Debug.Printf("@!! Loading asset " .. sName .. "." .. sAssetType .. " timed-out")
      else
        Debug.Printf("@ Loaded asset " .. sName .. "." .. sAssetType)
      end
      MrxUtil.CallWithOptionalArgs(fCallback, tCallbackData)
    else
      Debug.Printf("@ Already loaded asset " .. sName .. "." .. sAssetType .. " or timed-out")
    end
  end
  
  for k, v in pairs(tAssets) do
    local sKeyType = type(k)
    local sValType = type(v)
    if sValType == "string" then
      local sAssetName = v
      _nLoadPending = _nLoadPending + 1
      _tAssetLoadTimers[sAssetType] = _tAssetLoadTimers[sAssetType] or {}
      _tAssetLoadTimers[sAssetType][sAssetName] = Event.Create(Event.TimerRelative, {15, false}, _AssetLoaded, {sAssetName, true})
      _ProcessAsset(true, sAssetName, sAssetType, _AssetLoaded, {sAssetName, false})
    elseif sKeyType == "string" and sValType == "table" then
      local sName = k
      local tAssets = v
      if sName == MrxUtil.GetPrimaryCharacterName() or sName == "MaleStarter" or sName == "FemaleStarter" then
        _LoadTableOfAssets(sAssetType, tAssets, fCallback, tCallbackData)
      end
    end
  end
end

function _UnloadTableOfAssets(sAssetType, tAssets, fCallback, tCallbackData)
  _tAssetUnloadTimers = _tAssetUnloadTimers or {}
  
  local function _AssetUnloaded(sName, bTimerTriggered)
    local uTimer = _tAssetUnloadTimers[sAssetType][sName]
    if uTimer then
      Event.Delete(uTimer)
      _tAssetUnloadTimers[sAssetType][sName] = nil
      if bTimerTriggered then
        Debug.Printf("@!! Unloading asset " .. sName .. "." .. sAssetType .. " timed-out")
      else
        Debug.Printf("@ Unloaded asset " .. sName .. "." .. sAssetType)
      end
      MrxUtil.CallWithOptionalArgs(fCallback, tCallbackData)
    else
      Debug.Printf("@ Already unloaded asset " .. sName .. "." .. sAssetType .. " or timed-out")
    end
  end
  
  for k, v in pairs(tAssets) do
    local sKeyType = type(k)
    local sValType = type(v)
    if sValType == "string" then
      local sAssetName = v
      _nLoadPending = _nLoadPending + 1
      _tAssetUnloadTimers[sAssetType] = _tAssetUnloadTimers[sAssetType] or {}
      _tAssetUnloadTimers[sAssetType][sAssetName] = Event.Create(Event.TimerRelative, {15, false}, _AssetUnloaded, {sAssetName, true})
      _ProcessAsset(false, sAssetName, sAssetType, _AssetUnloaded, {sAssetName, false})
    elseif sKeyType == "string" and sValType == "table" then
      local sName = k
      local tAssets = v
      if sName == MrxUtil.GetPrimaryCharacterName() or sName == "MaleStarter" or sName == "FemaleStarter" then
        _UnloadTableOfAssets(sAssetType, tAssets, fCallback, tCallbackData)
      end
    end
  end
end

function _ProcessAsset(bLoad, sName, sType, fCallback, tCallbackData)
  local fProcess
  local sKey = sName .. "." .. sType
  _tLoadedAssets = _tLoadedAssets or {}
  if bLoad then
    if sType == "soundbank" or sType == "wavebank" then
      fProcess = MrxSoundBanks.LoadTempBank
    else
      fProcess = Pg.LoadAsset
    end
    local nRefs = _tLoadedAssets[sKey]
    if not nRefs then
      nRefs = 1
    else
      nRefs = nRefs + 1
    end
    Debug.Printf("@ Loading asset " .. sKey .. " (" .. nRefs .. ")")
    _tLoadedAssets[sKey] = nRefs
  else
    if sType == "soundbank" or sType == "wavebank" then
      fProcess = MrxSoundBanks.UnloadTempBank
    else
      fProcess = Pg.UnloadAsset
    end
    local nRefs = _tLoadedAssets[sKey]
    if not nRefs then
      Debug.Printf("@!! Attempting to unload unrequested asset " .. sKey)
    else
      nRefs = nRefs - 1
      Debug.Printf("@ Unloading asset " .. sKey .. " (" .. nRefs .. ")")
      if nRefs == 0 then
        nRefs = nil
      end
      _tLoadedAssets[sKey] = nRefs
    end
  end
  fProcess(sName, sType, fCallback, tCallbackData)
end

function _CheckAssets()
  Debug.Printf("@ Checking loaded assets...")
  local nAssets = 0
  for sKey, nRefs in pairs(_tLoadedAssets) do
    Debug.Printf("@!! Asset " .. sKey .. " still loaded with " .. nRefs .. " references!")
    nAssets = nAssets + 1
  end
  if nAssets == 0 then
    Debug.Printf("@ All assets unloaded!")
  end
  _tLoadedAssets = nil
end

function Start()
  _tBriefings = _oStarter:GetOfferedBriefings()
  for sMissionName, tMissionData in pairs(_tBriefings) do
    tMissionData.tConfig = WifBriefingData[sMissionName] or {}
  end
  _tMissionsToBeAccepted, _sLastAcceptedMission = _oStarter:GetMissionsToBeAccepted()
  if not _tMissionsToBeAccepted then
    _tMissionsToBeAccepted = {}
  end
  _nBaseShadowDistance = Graphics.GetShadowBaseDistance()
  Graphics.SetShadowBaseDistance(2)
  _SetDefaultCameraEffects()
  _bFadedIn = false
  MrxState.SetQuickFade(true)
  _SaveActorsOriginalPositions({"Player1", "Starter"})
  _AttachActorsToStartingLocations()
  _SetupPlayers(false)
  Gui.EnablePlayerMarkers(false)
  Hud.SubtitleBuffer:Clear({})
  Pda:SetSuppressed({bSuppress = true})
  local sFaction = _oStarter:GetFaction()
  if sFaction ~= "Pmc" then
    local sAttitude = MrxFactionManager.GetAttitudeLabel(sFaction, "Pmc")
    if sAttitude == "Hostile" then
      MrxFactionManager.SetRelation(sFaction, "Pmc", -33)
    end
  end
  if Net.IsServer() then
    Net.SendCustomEvent("MrxBriefing", NETEVENT_DISABLEMARKERS, {})
  end
  if not _oStarter:IsBoss() then
    Human.DisableWeapons(GetActorGuid("Starter"))
  end
  VO.Cancel()
  VO.SetCinematicMode(true)
  _BindFaceAnim(true, "Player1")
  _BindFaceAnim(true, "Starter")
  local sWagerMissionId, bWagerWin = WifPmcInterior.GetWagerStatus()
  if sWagerMissionId then
    Debug.Printf("################### Beginning Wager")
    _WagerBegin(sWagerMissionId, bWagerWin)
    return
  end
  _SetActorsToDefaultPose({"Player1", "Starter"}, false)
  if _oStarter:IsBoss() then
    _DisplayRootMenu()
  elseif _oStarter:IsPmcStarter() then
    if _HasSpecialCaseGreeting() then
      _SpecialCaseGreeting()
    else
      _ReturnToRootMenu()
    end
  else
    _Greeting()
  end
end

function _Greeting()
  local tCheapCinematic = _CreateCheapCinematic(CHEAP_GREETING)
  if tCheapCinematic then
    _ProcessCheapCinematic(tCheapCinematic, _BusinessCardMoment)
  else
    _BusinessCardMoment()
  end
end

function _BusinessCardMoment()
  _StopCheapCinematic()
  _DeleteSkipEvent()
  local tCardData = _oStarter:GetCardData()
  if tCardData and not _oStarter:HasCardBeenDisplayed() then
    _oStarter:CardDisplayed()
    Hud.CardFanfare:Commence({
      sFaction = tCardData.sFaction,
      sTitle = tCardData.sTitle,
      sName = tCardData.sName,
      sJobTitle = tCardData.sJobTitle,
      sPhone1 = tCardData.sPhone1,
      sPhone2 = tCardData.sPhone2,
      sEmail = "",
      nDisplayTime = 3,
      fCallback = _JobRequest
    })
  else
    Event.Create(Event.TimerRelative, {0}, _JobRequest)
  end
end

function _JobRequest()
  local tCheapCinematic = _CreateCheapCinematic(CHEAP_JOBREQUEST)
  if tCheapCinematic then
    _ProcessCheapCinematic(tCheapCinematic, _DisplayRootMenu)
  else
    _DisplayRootMenu()
  end
end

function _HasSpecialCaseGreeting()
  if _oStarter:GetSpecialCaseGreeting() then
    return true
  end
  return false
end

function _SpecialCaseGreeting()
  local tCheapCinematic = _CreateCheapCinematic(CHEAP_SPECIALCASEGREETING)
  if tCheapCinematic then
    _ProcessCheapCinematic(tCheapCinematic, _ReturnToRootMenu)
    return
  end
  _Fade(true)
  _DisplayRootMenu()
end

function _DisplayRootMenu()
  _StopCheapCinematic()
  _DeleteSkipEvent()
  local _tNames = {}
  local _tTitles = {}
  local _tActions = {}
  if _oStarter:HasIntros() then
    local tIntros = _oStarter:GetIntros()
    for sName, bViewed in pairs(tIntros) do
      local tIntro = WifBriefingData.Intros[sName]
      if tIntro then
        local sTitle = tIntro.sTitle
        if not bViewed then
          sTitle = "[yellow][shop.new] [white]" .. sTitle
        end
        table.insert(_tNames, sName)
        table.insert(_tTitles, sTitle)
        table.insert(_tActions, _PlayIntro)
      end
    end
  end
  for sMissionName, tMissionData in pairs(_tBriefings) do
    local nIndex = table.getn(_tNames) + 1
    if WifMissionData.IsMissionOnCriticalPath(sMissionName) then
      nIndex = 1
    end
    local sTitle = "\"" .. tMissionData.sTitle .. "\""
    if not _oStarter:IsBriefingOld(sMissionName) then
      sTitle = "[yellow][shop.new] [white]" .. sTitle
    end
    if tMissionData.sLevel then
      sTitle = sTitle .. " " .. tMissionData.sLevel
    end
    table.insert(_tNames, nIndex, sMissionName)
    table.insert(_tTitles, nIndex, sTitle)
    table.insert(_tActions, nIndex, _BriefingSelected)
  end
  if _oStarter:IsBoss() then
    _BriefingSelected(_tNames[1])
    return
  end
  if _oStarter:HasShop() and MrxFactionManager.GetPriceScale(_oStarter:GetFaction(), "Pmc") then
    table.insert(_tNames, "Shop")
    table.insert(_tTitles, "[Briefing.Shop]")
    table.insert(_tActions, _DisplayShop)
  end
  if _oStarter:HasTransitSystem() and MrxTransit.GetNumValidLocations() > 0 then
    table.insert(_tNames, "Transit")
    table.insert(_tTitles, "[Briefing.Transit]")
    table.insert(_tActions, _DisplayTransit)
  end
  if _oStarter:HasHintSystem() and WifHints.HasHint(_oStarter:GetPmcName()) then
    table.insert(_tNames, "Hint")
    table.insert(_tTitles, "[Briefing.Hint]")
    table.insert(_tActions, _DisplayHint)
  end
  if _oStarter:HasBribeSystem() then
    _tBribableFactions = MrxFactionManager.GetBribableFactions()
    if 0 < table.getn(_tBribableFactions) then
      table.insert(_tNames, "Bribe")
      table.insert(_tTitles, "[Briefing.Bribe]")
      table.insert(_tActions, _DisplayBribes)
    end
  end
  table.insert(_tNames, "Cancel")
  table.insert(_tTitles, "[Generic.Exit]")
  table.insert(_tActions, _Goodbye)
  if Net.IsServer() and not _ClientMenuBox then
    Net.SendCustomEvent("MrxBriefing", NETEVENT_DISPLAYMENU, {})
    _ClientMenuBox = true
  end
  LTILibName.ChangeShellState(true)
  MrxGui.DisplayDialogBox(Player.GetPrimaryPlayer(), "[Briefing.RootMenuDialog]", _tTitles, 1, _RootMenuOptionSelected, {_tNames, _tActions}, 48, 36, "left", "bottom", false, table.getn(_tTitles))
end

function _RootMenuOptionSelected(tNames, tActions, nIndex)
  local action = tActions[nIndex]
  if action then
    local name = tNames[nIndex]
    action(name)
  end
end

function _BriefingSelected(sName)
  _sSelectedMission = sName
  Debug.Printf("@@@@@@@@@@ _sSelectedMission = " .. _sSelectedMission)
  if _oStarter:IsMissionAccepted(_sSelectedMission) then
    _DisplayJobSummary()
  else
    if WifMissionData.IsMissionAContract(_sSelectedMission) then
      local bContractActive = false
      local bContractPending = false
      local sMessage
      if not MrxPlayState.IsFree() then
        bContractActive = true
        sMessage = "[Briefing.ContractActive]"
      end
      if _oStarter:IsPmcStarter() and WifPmcInterior.IsContractPending() or _oStarter:IsContractPending() then
        bContractPending = true
        sMessage = "[Briefing.ContractPending]"
      end
      if bContractActive or bContractPending then
        LTILibName.ChangeShellState(true)
        MrxGui.DisplayDialogBox(Player.GetPrimaryPlayer(), sMessage, {
          "[Generic.Back]"
        }, 1, _DisplayRootMenu, {}, 48, 36, "left", "bottom", false, 1)
        return
      end
    end
    
    local function _BeginLoad()
      local tConfig = _GetSelectedBriefingConfig()
      if tConfig.tPositions then
        _AttachActorsToHardpoints(tConfig.tPositions)
      end
      _LoadSpiel()
    end
    
    _Fade(false, _BeginLoad)
  end
end

function _LoadSpiel()
  if Net.IsServer() then
    local idx = 1
    local tNetBriefingActors = {}
    if _sSelectedMission then
      local tBriefingData = WifBriefingData[_sSelectedMission]
      if tBriefingData then
        local tActors = tBriefingData.tActors
        if tActors then
          for sName, tActorData in pairs(tActors) do
            if sName ~= "Starter" then
              tNetBriefingActors[idx] = GetActorGuid(sName)
              Debug.Printf("WifBriefingData[ " .. tostring(_sSelectedMission) .. " ].tActor[ " .. tostring(sName) .. " ] = " .. tostring(tNetBriefingActors[idx]))
              idx = idx + 1
            end
          end
        end
      end
    end
    Net.LoadMissionSpiel(WifMissionData.GetMissionIndexFromId(_sSelectedMission), tNetBriefingActors)
  end
  local sSpielFile = GetSpielFileName(_sSelectedMission)
  Debug.Printf("Loading briefing spiel " .. sSpielFile .. "...")
  dynamic_import(sSpielFile, _FileLoaded)
end

function _FileLoaded(tFile)
  local tConfig = _GetSelectedBriefingConfig()
  MrxUtil.SetupLoadingCallback(_THIS, _StartSpiel)
  _nLoadPending = _nLoadPending + 1
  local tData = {_THIS}
  if type(tFile) == "table" then
    tConfig.tCheapCinematic = {
      tSequence = tFile.tSequence,
      sParticipant1 = tFile.sParticipant1,
      sParticipant2 = tFile.sParticipant2,
      tParticipant1CamOverride = tFile.tParticipant1CamOverride,
      tParticipant2CamOverride = tFile.tParticipant2CamOverride
    }
    for i, vFrame in ipairs(tFile.tSequence) do
      if type(vFrame) == "table" and vFrame.sFlashFile then
        local sName = vFrame.sFlashFile
        local tPosition = vFrame.tPosition
        if sName then
          _nLoadPending = _nLoadPending + 1
          _AddFlashObject(sName, tPosition, MrxUtil.LoadingCallback, tData)
        end
      end
    end
  end
  if tConfig.tAssetPreload then
    for sAssetType, tAssets in pairs(tConfig.tAssetPreload) do
      _LoadTableOfAssets(sAssetType, tAssets, MrxUtil.LoadingCallback, tData)
    end
  end
  if tConfig.tFaceAnimSets then
    for sActor, vSet in pairs(tConfig.tFaceAnimSets) do
      local sSet = GetAnimSet(vSet, sActor)
      if sSet then
        _nLoadPending = _nLoadPending + 1
        _ProcessAsset(true, sSet, "facefxanimationset", MrxUtil.LoadingCallback, tData)
      end
    end
  end
  local uChar1 = Player.GetPrimaryCharacter()
  if uChar1 ~= nil and not Object.IsAwake(uChar1) then
    Debug.Printf("Waiting for Player1 to unhibernate...")
    _nLoadPending = _nLoadPending + 1
    Event.Create(Event.ObjectIsReady, {uChar1}, MrxUtil.LoadingCallback, tData)
  end
  local uChar2 = Player.GetSecondaryCharacter()
  if uChar2 ~= nil and not Object.IsAwake(uChar2) then
    Debug.Printf("Waiting for Player2 to unhibernate...")
    _nLoadPending = _nLoadPending + 1
    Event.Create(Event.ObjectIsReady, {uChar2}, MrxUtil.LoadingCallback, tData)
  end
  if tConfig.tCinematic then
    local tCinematic = tConfig.tCinematic[MrxUtil.GetPrimaryCharacterName()]
    ASSERT(tCinematic)
    for _, tFrame in ipairs(tCinematic) do
      if tFrame.tAnims then
        for sActor, sFile in pairs(tFrame.tAnims) do
          _nLoadPending = _nLoadPending + 1
          _ProcessAsset(true, sFile, "animation", MrxUtil.LoadingCallback, tData)
        end
      end
      if tFrame.tFlash then
        local sName = tFrame.tFlash.sFile
        local tPosition = tFrame.tFlash.tPosition
        if sName then
          _nLoadPending = _nLoadPending + 1
          _AddFlashObject(sName, tPosition, MrxUtil.LoadingCallback, tData)
        end
      end
    end
  end
  local tRewards = MrxRewardData.GetRewards(_sSelectedMission)
  local bHasWager = tRewards and (tRewards.nWager or tRewards.nWagerPercent)
  if not Net.IsClient() and _oStarter:IsPmcStarter() and bHasWager then
    tConfig.tCheapCinematic = _CreateCheapCinematic(CHEAP_PMCWAGER)
    if tConfig.tSlides then
      tConfig.tSlides = nil
    end
  end
  MrxUtil.LoadingCallback(_THIS)
end

function _StartSpiel()
  if Net.IsClient() then
    if not _tBriefings then
      return
    end
    _ProcessCameraSettings({
      tShot = {
        sName = "OverTheShoulderLeft",
        sBaseActor = "Starter",
        sTargetActor = "Player1"
      }
    })
    _bNetSafeSpielLoaded = true
  end
  local tConfig = _GetSelectedBriefingConfig()
  Debug.Printf("Starting briefing " .. _sSelectedMission)
  _nCinematicFrame = nil
  _nSlideFrame = nil
  if tConfig.tFaceAnimSets then
    for sActor, vSet in pairs(tConfig.tFaceAnimSets) do
      local sSet = GetAnimSet(vSet, sActor)
      if sSet then
        _BindFaceAnim(false, sActor)
        _BindFaceAnim(true, sActor, sSet)
      end
    end
  end
  local tCinematic
  if tConfig.tCinematic then
    tCinematic = tConfig.tCinematic[MrxUtil.GetPrimaryCharacterName()]
    local tCameraEffects
    if tConfig.tCinematic.tCameraEffects then
      tCameraEffects = tConfig.tCinematic.tCameraEffects[MrxUtil.GetPrimaryCharacterName()]
      if tCameraEffects then
        _ProcessCameraEffects(tConfig, tCameraEffects)
      end
    end
    _ProcessCameraSettings({
      sPositionObject = "Player1",
      sPositionHardpoint = "Bone_Attach_Root",
      sLookAtObject = "Player1",
      sLookAtHardpoint = "Bone_Attach_Root",
      bLookAtDirection = true
    })
    if Animation.SetUseBriefingLOD then
      Animation.SetUseBriefingLOD(true)
    end
  end
  
  local function _PlayCinematic()
    if tConfig.tSlides then
      Debug.Printf("Briefing mode: user-defined slides")
      if not Net.IsClient() then
        MrxCinematic.PlaceholderSequence(tConfig.tSlides, _CinematicComplete)
      end
    elseif tCinematic then
      Debug.Printf("Briefing mode: cinematic")
      _NextCinematicFrame(tCinematic, _CinematicComplete)
      _CreateSkipEvent(_CinematicComplete)
    elseif tConfig.tCheapCinematic then
      Debug.Printf("Briefing mode: cheap cinematic")
      if _oStarter:IsBoss() or _oStarter:IsPmcStarter() then
        tConfig.tCheapCinematic.bFadeIn = not _bFadedIn
        _ProcessCheapCinematic(tConfig.tCheapCinematic, _CinematicComplete)
      else
        if not _bFadedIn then
          _Fade(true)
        end
        _CinematicComplete()
      end
    elseif not Net.IsClient() then
      Debug.Printf("Briefing mode: placeholder slides")
      MrxCinematic.PlaceholderSequence({
        {
          sCaption = [[
Mission Briefing:
"]] .. _sSelectedMission .. "\""
        }
      }, _CinematicComplete)
    end
  end
  
  if not tCinematic and not tConfig.tCheapCinematic and not _bFadedIn then
    _Fade(true, _PlayCinematic)
  else
    _PlayCinematic()
  end
end

function _DisplayConfirmDialog()
  local tConfig = _GetSelectedBriefingConfig()
  if tConfig.tCinematic then
    _ProcessCameraSettings({
      sPositionObject = "HqInterior",
      sPositionHardpoint = "hp_menu_camera",
      sLookAtObject = "HqInterior",
      sLookAtHardpoint = "hp_menu_camera",
      bLookAtDirection = true
    })
  end
  if Net.IsClient() then
    if not _ClientShopAvailable then
      if _ClientWaitBox then
        _ClientWaitBox:Close()
        _ClientWaitBox = nil
      end
      _ClientWaitBox = MrxGui.DisplayDialogBox(Player.GetLocalPlayer(), "[Generic.WaitingForHostPlayer]", {}, 1, function()
        _ClientWaitBox = nil
      end, {}, 48, 36, "left", "bottom", false)
    end
    return
  end
  local tRewards = MrxRewardData.GetRewards(_sSelectedMission)
  local bWager = false
  if tRewards then
    local tWagerData = MrxRewardData.GetWagerData(tRewards)
    if tWagerData then
      bWager = true
      Hud.ResourceCounter:Show({nDuration = -1})
      if not tRewards.nWagered then
        tRewards.nWagered = tWagerData.nDefaultWager
      end
    end
  end
  local sMessageText = WifMissionFlow.BuildMissionDescription(_sSelectedMission, true, false)
  LTILibName.ChangeShellState(true)
  MrxGuiDialogBox.DisplayScrollingDialogBox(Player.GetPrimaryPlayer(), sMessageText, _HandleConfirmDialogInput, {}, bWager)
end

function _HandleConfirmDialogInput(nIndex)
  if nIndex == 1 then
    if WifRecommendationData.HasRecommendations(_sSelectedMission) then
      _DisplayRecommendationsDialog()
    else
      _AcceptOrDeclineMission(true)
    end
  elseif nIndex == 2 then
    _AcceptOrDeclineMission(false)
  elseif nIndex == 3 then
    _DisplayWagerDialog()
  end
end

function _DisplayRecommendationsDialog()
  local sMessageText, bAllInStock = WifRecommendationData.GenerateRecommendationString(_sSelectedMission)
  local tCues
  local sCue = false
  if bAllInStock == true then
    tCues = {
      "Fiona.vo2fio22",
      "Fiona.aa3fio03"
    }
  elseif bAllInStock == false then
    tCues = {
      "Fiona.Cam.63",
      "Fiona.Cam.64"
    }
  end
  if tCues then
    sCue = MrxUtil.GetRandomTableElement(tCues)
    Sound.CueSound(0, sCue)
  end
  sMessageText = sMessageText and "[PDA.Map.RecommendationsHeader]\n" .. sMessageText
  MrxGuiDialogBox.DisplayScrollingDialogBox(Player.GetPrimaryPlayer(), sMessageText, _HandleRecommendationsDialogInput, {sCue}, false)
end

function _HandleRecommendationsDialogInput(sCue, nIndex)
  if sCue then
    Sound.StopSound(0, sCue)
  end
  if nIndex == 1 then
    _AcceptOrDeclineMission(true)
  elseif nIndex == 2 then
    _DisplayConfirmDialog()
  end
end

function _AcceptOrDeclineMission(bAccepted)
  local tConfig = _GetSelectedBriefingConfig()
  local tCameraEffects
  Hud.ResourceCounter:Hide({})
  if bAccepted then
    if WifMissionData.IsMissionAContract(_sSelectedMission) then
      _oStarter:SetPendingContract(_sSelectedMission)
    end
    _oStarter:SetMissionAccepted(_sSelectedMission, true)
    table.insert(_tMissionsToBeAccepted, _sSelectedMission)
    _sLastAcceptedMission = _sSelectedMission
  end
  if _oStarter:IsBoss() or tConfig.nType == WifBriefingData.knSimple then
    local tCinematic
    local fNext = _End
    if bAccepted and tConfig.tConfirmCinematic then
      tCinematic = tConfig.tConfirmCinematic
      if Net.IsServer() then
        Net.SetBriefingCheapCinematic(CHEAP_CONFIRM)
      end
    elseif not bAccepted and tConfig.tDeclineCinematic then
      tCinematic = tConfig.tDeclineCinematic
      if Net.IsServer() then
        Net.SetBriefingCheapCinematic(CHEAP_DECLINE)
      end
    end
    if tCinematic then
      local sCharName = MrxUtil.GetPrimaryCharacterName()
      if tCinematic.tCameraEffects then
        _ProcessCameraEffects(tConfig, tCinematic.tCameraEffects[sCharName])
      end
      _NextCinematicFrame(tCinematic[sCharName], fNext)
      _CreateSkipEvent(fNext)
    else
      fNext()
    end
  elseif not _oStarter:IsPmcStarter() then
    local fNext = _ReturnToRootMenu
    if bAccepted then
      fNext = _End
    end
    local tCheapCinematic
    if bAccepted then
      tCheapCinematic = _CreateCheapCinematic(CHEAP_JOBACCEPT)
    else
      tCheapCinematic = _CreateCheapCinematic(CHEAP_JOBDECLINE)
    end
    if tCheapCinematic then
      _ProcessCheapCinematic(tCheapCinematic, fNext)
    else
      fNext()
    end
  elseif bAccepted then
    _End()
  else
    _ReturnToRootMenu()
  end
end

function _ReturnToRootMenu()
  _StopCheapCinematic()
  _DeleteSkipEvent()
  _UnloadSpiel()
  _Fade(false, _PrepareForRootMenu)
end

function _PrepareForRootMenu()
  Hud.SubtitleBuffer:Clear({})
  _ProcessCameraSettings({
    tShot = {
      sName = "OverTheShoulderLeft",
      sBaseActor = "Starter",
      sTargetActor = "Player1"
    }
  })
  _SetDefaultCameraEffects()
  _AttachActorsToStartingLocations()
  _SetActorsToDefaultPose({"Player1", "Starter"})
  Event.Create(Event.TimerRelative, {0.5}, _Fade, {true, _DisplayRootMenu})
end

function _DisplayWagerDialog(nValue)
  local tMissionData = _tBriefings[_sSelectedMission]
  local tRewards = MrxRewardData.GetRewards(_sSelectedMission)
  local tWagerData = MrxRewardData.GetWagerData(tRewards)
  local nMaxDigit = MrxUtil.GetNumberOfDigits(tWagerData.nWagerMax)
  local nMinDigit = 3
  local nStartDigit = MrxUtil.GetNumberOfDigits(nValue or tWagerData.nDefaultWager)
  Debug.Printf("Wager - Min: " .. nMinDigit .. " Max: " .. nMaxDigit .. " Start: " .. nStartDigit)
  local nWagerLimitMin = tWagerData.nWagerMin
  local nWagerLimitMax = tWagerData.nWagerMax
  if nWagerLimitMax > tWagerData.nCash then
    if nWagerLimitMin > tWagerData.nCash then
      nWagerLimitMax = nWagerLimitMin
    else
      nWagerLimitMax = tWagerData.nCash
    end
  end
  local sMessageText = "[Briefing.ChangeWagerQuery]"
  local sPostFixMessage = ""
  if tWagerData.nWagerMin then
    local sWagerMinText = MrxUtil.FormatMoney(tWagerData.nWagerMin)
    sPostFixMessage = sPostFixMessage .. "[Briefing.MinWagerPrefix] " .. sWagerMinText
  end
  if tWagerData.nWagerMax then
    local sWagerMaxText = MrxUtil.FormatMoney(tWagerData.nWagerMax)
    sPostFixMessage = sPostFixMessage .. " [Briefing.MaxWagerPrefix] " .. sWagerMaxText
  end
  if sPostFixMessage ~= "" then
    sPostFixMessage = "(" .. sPostFixMessage .. ")"
  else
    sPostFixMessage = nil
  end
  local sLanguage = "English"
  if Sys.GetLanguage then
    sLanguage = Sys.GetLanguage() or "English"
  end
  local sPrefix = "$"
  local sSuffix = ".0 [Generic.Money.Thousand]"
  if "English" == sLanguage then
    sPrefix = "$"
    sSuffix = ".0 [Generic.Money.Thousand]"
  elseif "French" == sLanguage then
    sPrefix = " "
    sSuffix = ",0 [Generic.Money.Thousand] $"
  elseif "German" == sLanguage then
    sPrefix = " "
    sSuffix = ",0 [Generic.Money.Thousand] $"
  elseif "Italian" == sLanguage then
    sPrefix = "$"
    sSuffix = ",0 [Generic.Money.Thousand]"
  elseif "Spanish" == sLanguage then
    sPrefix = " "
    sSuffix = ",0 [Generic.Money.Thousand] $"
  elseif "Russian" == sLanguage then
    sPrefix = "$"
    sSuffix = ",0 [Generic.Money.Thousand] $"
  end
  LTILibName.ChangeShellState(true)
  MrxGui.DisplayNumericBox(Player.GetPrimaryPlayer(), sMessageText, sPostFixMessage, sPrefix, sSuffix, nValue or tWagerData.nDefaultWager, nWagerLimitMin, nWagerLimitMax, nStartDigit, nMinDigit, nMaxDigit, _DisplayConfirmWagerDialog, {}, _DisplayCancelWagerDialog, {}, 48, 36, "right", "bottom", false)
end

function _DisplayConfirmWagerDialog(nValue)
  MrxGui.DisplayDialogBox(Player.GetPrimaryPlayer(), string.format([[
[Briefing.WagerConfirmDialog]
%s]], MrxUtil.FormatMoney(nValue)), {
    "[Generic.Confirm]",
    "[Generic.Cancel]"
  }, 1, function(nValue, nIndex)
    if nIndex == 1 then
      local tRewards = MrxRewardData.GetRewards(_sSelectedMission)
      if tRewards then
        tRewards.nWagered = nValue
      end
      _DisplayConfirmDialog()
    else
      _DisplayWagerDialog(nValue)
    end
  end, {nValue}, 48, 36, "right", "bottom", false, 2)
end

function _DisplayCancelWagerDialog(nValue)
  MrxGui.DisplayDialogBox(Player.GetPrimaryPlayer(), "[Briefing.WagerCancelDialog]", {
    "[Generic.Yes]",
    "[Generic.No]"
  }, 2, function(nIndex)
    if nIndex == 1 then
      _DisplayConfirmDialog()
    else
      _DisplayWagerDialog(nValue)
    end
  end, {}, 48, 36, "right", "bottom", false, 2)
end

function _Goodbye()
  if Net.IsServer() and _ClientMenuBox then
    Net.SendCustomEvent("MrxBriefing", NETEVENT_HIDEMENU, {})
    _ClientMenuBox = nil
  end
  local tCheapCinematic = _CreateCheapCinematic(CHEAP_GOODBYE)
  if tCheapCinematic then
    _ProcessCheapCinematic(tCheapCinematic, _End)
  else
    _End()
  end
end

function _End()
  if Net.IsServer() then
    Net.SetLoadingScreen(true)
  end
  Debug.Printf("Ending Cinematic")
  _StopCheapCinematic()
  _DeleteSkipEvent()
  _UnloadSpiel(true)
  if _ClientMenuBox then
    if Net.IsClient() then
      Debug.Printf("... I see the client menu is still open, closing it now ...")
      _CleanupClientMenu()
    else
      _ClientMenuBox = nil
    end
  end
  if _ClientWaitBox then
    _ClientWaitBox:Close()
    _ClientWaitBox = nil
  end
  _ClientMenuPending = nil
  LTILibName.ChangeShellState(false)
  MrxState.SetQuickFade(false)
  _Fade(false, _EndBegin)
end

function _EndBegin()
  Graphics.SetShadowBaseDistance(_nBaseShadowDistance)
  _SetCameraEffects(0, "RestoreAll", 1)
  _BindFaceAnim(false, "Player1")
  _BindFaceAnim(false, "Starter")
  _SetupPlayers(true)
  Gui.EnablePlayerMarkers(true)
  Pda:SetSuppressed({bSuppress = false})
  if Net.IsServer() then
    Net.SendCustomEvent("MrxBriefing", NETEVENT_ENABLEMARKERS, {})
  end
  if not Net.IsClient() then
    _DetachActorsFromHardpoints({"Player1", "Starter"})
  else
    _DetachActorsFromHardpoints({"Player1"})
  end
  if _oStarter:IsPmcStarter() then
    _RestoreActorsToOriginalPositions({"Player1", "Starter"})
  end
  VO.SetCinematicMode(false)
  if Animation.SetUseBriefingLOD then
    Animation.SetUseBriefingLOD(false)
  end
  if Net.IsClient() then
    MrxState.Exit(MrxState.STATE_WAITFORGAME)
  else
    _oStarter:End(_tMissionsToBeAccepted, _sLastAcceptedMission)
  end
  if Net.IsServer() then
    Net.SetBriefingStarters(0)
  end
end

function _DisplayJobSummary()
  local tMissionData = _tBriefings[_sSelectedMission]
  local sTitle = tMissionData.sTitle
  local sJobSummary
  if tMissionData.sLevel then
    sJobSummary = "\"" .. sTitle .. "\" " .. tMissionData.sLevel .. [[


]] .. "[" .. _sSelectedMission .. ".Terms.Summary]\n"
  else
    sJobSummary = "\"" .. sTitle .. [[
"

]] .. "[" .. _sSelectedMission .. ".Terms.Summary]\n"
  end
  LTILibName.ChangeShellState(true)
  MrxGui.DisplayDialogBox(Player.GetPrimaryPlayer(), sJobSummary, {
    "[Generic.Back]"
  }, 1, _DisplayRootMenu, {}, 48, 36, "left", "bottom", false, 1)
end

function _DisplayClientMenu()
  if not _oStarter then
    _ClientMenuPending = true
    Debug.Printf("DisplayClientMenu(): Waiting for starter to be loaded!")
    return
  end
  if _ClientMenuPending then
    Debug.Printf("DisplayClientMenu(): Stater now loaded, proceeding.")
    _ClientMenuPending = nil
  end
  if _oStarter:IsBoss() then
    Debug.Printf("DisplayClientMenu(): Stater is a boss!")
    return
  end
  if _oStarter:HasShop() and MrxFactionManager.GetPriceScale(_oStarter:GetFaction(), "Pmc") then
    _ClientShopAvailable = true
    LTILibName.ChangeShellState(true)
    _ClientMenuBox = MrxGui.DisplayDialogBox(Player.GetLocalPlayer(), "[Briefing.RootMenuDialog]", {
      "[Briefing.Shop]"
    }, 1, _DisplayShop, {}, 48, 36, "left", "bottom", false)
  else
    Debug.Printf("DisplayClientMenu(): Stater either has no shop or is hostile!")
  end
end

function _CleanupClientMenu()
  _ClientShopAvailable = nil
  Debug.Printf("Client Cleanup: Closing store.")
  MrxShop.Close()
  if _ClientMenuBox then
    Debug.Printf("Client Cleanup: Closing client menu.")
    _ClientMenuBox:Close()
    _ClientMenuBox = nil
  end
end

function _DisplayShop()
  local fOnShopClose
  LTILibName.ChangeShellState(false)
  if Net.IsClient() then
    fOnShopClose = _DisplayClientMenu
  else
    fOnShopClose = _DisplayRootMenu
  end
  MrxShop.Open(_oStarter, fOnShopClose)
end

function _DisplayHint()
  local tCheapCinematic = _CreateCheapCinematic(CHEAP_HINT)
  _ProcessCheapCinematic(tCheapCinematic, _ReturnToRootMenu)
end

function _DisplayBribes()
  local tOptions = {}
  for i, sFactionAbbrev in ipairs(_tBribableFactions) do
    local sFactionName = MrxFactionManager.GetPlayerVisibleName(sFactionAbbrev)
    table.insert(tOptions, sFactionName)
  end
  table.insert(tOptions, "[Generic.Back]")
  LTILibName.ChangeShellState(true)
  MrxGui.DisplayDialogBox(Player.GetPrimaryPlayer(), "[Briefing.BribeMenuDialog]", tOptions, 1, _ConfirmBribe, {}, 48, 36, "left", "bottom", false, table.getn(tOptions))
end

function _ConfirmBribe(nIndex)
  if nIndex > table.getn(_tBribableFactions) then
    _DisplayRootMenu()
  else
    local sFactionAbbrev = _tBribableFactions[nIndex]
    local sFactionName = MrxFactionManager.GetPlayerVisibleName(sFactionAbbrev)
    local nCash = MrxPmc.GetCashQty()
    local nBribe = math.floor(nCash * 0.2)
    local sBribeText = MrxUtil.FormatMoney(nBribe)
    local sBribeDialog = "[Briefing.BribeConfirmQuery:" .. sFactionName .. "]" .. " (" .. sBribeText .. ")"
    LTILibName.ChangeShellState(true)
    MrxGui.DisplayDialogBox(Player.GetPrimaryPlayer(), sBribeDialog, {
      "[Generic.Confirm]",
      "[Generic.Cancel]"
    }, 1, _ExecuteBribe, {sFactionAbbrev, nBribe}, 48, 36, "left", "bottom", false, 2)
  end
end

function _ExecuteBribe(sFactionAbbrev, nBribe, nIndex)
  if nIndex == 1 then
    MrxPmc.AddCashQty(-nBribe, nil, "[Generic.Bribes]")
    local nRelation = MrxFactionManager.GetAttitudeMedianValue("Friendly")
    MrxFactionManager.SetRelation(sFactionAbbrev, "Pmc", nRelation)
    _DisplayRootMenu()
  else
    _DisplayRootMenu()
  end
end

function _DisplayTransit()
  MrxTransit.OpenInterface(Player.GetLocalPlayer(), _TransitCallback)
end

function _TransitCallback(nSelectedIndex, bSuccess)
  if not bSuccess then
    _DisplayRootMenu()
    return
  end
  _End()
  MrxPmc.AddFuelQty(-1 * math.min(MrxTransit.GetTransitFuelCost(), MrxPmc.GetFuelQty()))
  MrxSupportTransit:TransitInterfaceCallbackBriefing(nSelectedIndex)
end

function _WagerBegin(sMissionId, bWagerWin)
  local uActor = GetActorGuid("Player1")
  local uLocation = Pg.GetGuidByName("Wager Exit " .. _oStarter:GetPmcName() .. " 1")
  local x, y, z = Object.GetPosition(uLocation)
  local yaw = Object.GetYaw(uLocation)
  _tOriginalActorPositions[uActor] = {
    x,
    y,
    z,
    yaw
  }
  local tRewards = MrxRewardData.GetRewards(sMissionId)
  if not tRewards or not tRewards.nWagered then
    Debug.Printf("################### No wager found for mission " .. sMissionId)
    _WagerEnd()
    return
  end
  _nWager = tRewards.nWagered
  _sWagerMissionId = sMissionId
  _bWagerWin = bWagerWin
  Debug.Printf("Beginning WagerComplete Cinematic")
  local tCheapCinematic
  if _bWagerWin then
    tCheapCinematic = _CreateCheapCinematic(CHEAP_WAGERBEGINWIN)
  else
    tCheapCinematic = _CreateCheapCinematic(CHEAP_WAGERBEGINLOSE)
  end
  _ProcessCheapCinematic(tCheapCinematic, _WagerTransaction)
end

function _WagerTransaction()
  _StopCheapCinematic()
  _DeleteSkipEvent()
  Event.Create(Event.TimerRelative, {0.1}, _ExecuteWagerTransaction, {_bWagerWin, 1})
end

function _ExecuteWagerTransaction(bWagerWin, nChoice)
  local tVo
  local bChickenSuit = false
  if bWagerWin then
    MrxPmc.DisplayCash(MrxPmc.GetCashQty(), "[Generic.Wagers]", _nWager)
  elseif nChoice == 1 then
    MrxPmc.DisplayCash(MrxPmc.GetCashQty(), "[Generic.Wagers]", -_nWager)
  else
    bChickenSuit = true
  end
  local tRewards = MrxRewardData.GetRewards(_sWagerMissionId)
  tRewards.nWagered = nil
  
  local function _PostTransactionVo()
    local tCheapCinematic
    if bWagerWin then
      tCheapCinematic = _CreateCheapCinematic(CHEAP_WAGERWON)
    elseif nChoice == 1 then
      tCheapCinematic = _CreateCheapCinematic(CHEAP_WAGERLOST)
    else
      tCheapCinematic = _CreateCheapCinematic(CHEAP_WAGERCHICKENSUIT)
    end
    
    local function fComplete()
      _StopCheapCinematic()
      _DeleteSkipEvent()
      _WagerEnd()
    end
    
    _ProcessCheapCinematic(tCheapCinematic, fComplete)
  end
  
  if bChickenSuit then
    WifPmcInterior.ChangeOutfit(Player.GetLocalCharacter(), "Chicken Suit", _PostTransactionVo)
  else
    _PostTransactionVo()
  end
end

function _WagerEnd()
  _nWager = nil
  _sWagerMissionId = nil
  _bWagerWin = nil
  _ReturnToRootMenu()
end

function _PlayIntro(sName)
  local tIntro = WifBriefingData.Intros[sName]
  if not tIntro then
    if not Net.IsClient() then
      _DisplayRootMenu()
    end
    return
  end
  _Fade(false, _StartIntro, {sName})
end

function _StartIntro(sName)
  local tIntro = WifBriefingData.Intros[sName]
  local tCheapCinematic = {
    sParticipant1 = "Player1",
    sParticipant2 = "Starter",
    bFadeIn = true,
    tSequence = tIntro.tSequence
  }
  
  local function _IntroComplete()
    _ClearAllFlashObjects()
    if not Net.IsClient() then
      if not _oStarter:HasViewedIntro(sName) then
        _tViewedIntros[sName] = true
      end
      _oStarter:SetViewedIntro(sName, true, true)
      _ReturnToRootMenu()
    end
  end
  
  if Net.IsServer() then
    Net.SetBriefingCheapCinematic(CHEAP_INTRO, WifBriefingData.GetIntroIndexById(sName))
  end
  _ProcessCheapCinematic(tCheapCinematic, _IntroComplete)
end

function _UnloadSpiel(bExitingBriefing)
  if not _sSelectedMission then
    return
  end
  Debug.Printf("Unloading Cinematic " .. _sSelectedMission)
  if Net.IsServer() then
    Net.UnloadMissionSpiel(bExitingBriefing)
  end
  local tConfig = _GetSelectedBriefingConfig()
  if tConfig.tFaceAnimSets then
    for sActor, vSet in pairs(tConfig.tFaceAnimSets) do
      local sSet = GetAnimSet(vSet, sActor)
      if sSet then
        _BindFaceAnim(false, sActor, sSet)
        _BindFaceAnim(true, sActor)
      end
    end
  end
  if _nLoadPending == nil then
    MrxUtil.SetupLoadingCallback(_THIS)
  end
  _nLoadPending = _nLoadPending + 1
  if tConfig.tAssetPreload then
    for sAssetType, tAssets in pairs(tConfig.tAssetPreload) do
      _UnloadTableOfAssets(sAssetType, tAssets, MrxUtil.LoadingCallback, {_THIS})
    end
  end
  MrxUtil.LoadingCallback(_THIS)
  if tConfig.tFaceAnimSets then
    for sActor, vSet in pairs(tConfig.tFaceAnimSets) do
      local sSet = GetAnimSet(vSet, sActor)
      if sSet then
        _ProcessAsset(false, sSet, "facefxanimationset")
      end
    end
  end
  if tConfig.tCinematic then
    local tCinematic = tConfig.tCinematic[MrxUtil.GetPrimaryCharacterName()]
    for _, tFrame in ipairs(tCinematic) do
      if tFrame.tAnims then
        for sActor, sFile in pairs(tFrame.tAnims) do
          local uGuid = GetActorGuid(sActor)
          if uGuid and not Object.HasLabel(uGuid, "Human") then
            Object.StopAllAnimation(uGuid)
          end
          _ProcessAsset(false, sFile, "animation")
        end
      end
    end
  end
  if tConfig.tConfirmCinematic then
    _CleanupCinematic(tConfig.tConfirmCinematic)
  end
  if tConfig.tDeclineCinematic then
    _CleanupCinematic(tConfig.tDeclineCinematic)
  end
  if Net.IsClient() and tConfig.tCinematic then
    _CleanupCinematic(tConfig.tCinematic)
  end
  local sSpielFile = GetSpielFileName(_sSelectedMission)
  dynamic_remove(sSpielFile)
  _sSelectedMission = nil
end

function _NextCinematicFrame(tCinematic, fCallback, tCallbackArgs)
  local tConfig = _GetSelectedBriefingConfig()
  if not tCinematic then
    MrxUtil.CallWithOptionalArgs(fCallback, tCallbackArgs)
    return
  end
  local bFirstFrame = false
  if _nCinematicFrame then
    _nCinematicFrame = _nCinematicFrame + 1
  else
    _nCinematicFrame = 1
    bFirstFrame = true
  end
  Debug.Printf("Playing Cinematic - Cinematic Frame " .. _nCinematicFrame)
  local tFrame = tCinematic[_nCinematicFrame]
  if not tFrame then
    MrxUtil.CallWithOptionalArgs(fCallback, tCallbackArgs)
    return
  end
  local tNextFrame = tCinematic[_nCinematicFrame + 1]
  if tFrame.tAnims then
    if bFirstFrame == true then
      Debug.Printf("First-frame animation event set")
      Event.Create(Event.AnimationEvent, {
        0,
        "CinematicStart"
      }, _Fade, {true})
      bFirstFrame = nil
    end
    for sActor, sAnim in pairs(tFrame.tAnims) do
      local uGuid = GetActorGuid(sActor)
      if Object.HasLabel(uGuid, "Human") then
        Debug.Printf(sActor .. " playing " .. sAnim .. " (Human)")
        Human.PlayRawAnimation(uGuid, sAnim, false, false, -1, true, true)
      else
        Debug.Printf(sActor .. " playing " .. sAnim .. " (Non-Human)")
        Object.PlayAnimation(uGuid, sAnim, false, nil, 0, true)
      end
    end
  end
  if tFrame.tFlash then
    _tFlashTimers = _tFlashTimers or {}
    local iTimer = table.getn(_tFlashTimers)
    if tFrame.tFlash.sFile ~= nil then
      _ShowFlashObject(tFrame.tFlash.sFile)
      _tFlashTimers[iTimer] = Event.Create(Event.TimerRelative, {
        tFrame.tFlash.nTime
      }, _RemoveFlashObject, {
        tFrame.tFlash.sFile,
        iTimer
      })
    else
      for vKey, vData in pairs(tFrame.tFlash) do
        _ShowFlashObject(vData.sFile)
        _tFlashTimers[iTimer] = Event.Create(Event.TimerRelative, {
          vData.nTime
        }, _RemoveFlashObject, {
          vData.sFile,
          iTimer
        })
        iTimer = iTimer + 1
      end
    end
  end
  if tFrame.tCamera then
    _ProcessCameraSettings(tFrame.tCamera)
  end
  if bFirstFrame == true and not _bFadedIn then
    Debug.Printf("First-frame fade-in immediate")
    _Fade(true)
  end
  if tFrame.OnTime then
    Debug.Printf("OnTime - " .. tFrame.OnTime)
    local nTime = tFrame.OnTime
    if not tFrame.tAnims and tFrame.tFlash and tNextFrame.tAnims then
      local nShift = 1.8
      Debug.Printf("OnTime - Special-case - shifting time from " .. nTime .. " to " .. nTime - nShift)
      nTime = nTime - nShift
    end
    
    local function _OnTimer()
      Debug.Printf("OnTimer")
      tFrame._OnTimeEvent = nil
      if _nCinematicFrame then
        _NextCinematicFrame(tCinematic, fCallback, tCallbackArgs)
      else
        Debug.Printf("Residual OnTime event...ignoring")
      end
    end
    
    tFrame._OnTimeEvent = Event.Create(Event.TimerRelative, {
      tFrame.OnTime,
      false
    }, _OnTimer)
  end
  if tFrame.OnComplete then
    Debug.Printf("OnComplete - " .. tFrame.OnComplete)
    
    local function _OnComplete(nEvent)
      if nEvent == 1 then
        Debug.Printf("OnComplete - Event")
        Event.Delete(tFrame._OnCompleteTransition)
      else
        Debug.Printf("OnComplete - Transition")
        Event.Delete(tFrame._OnCompleteEvent)
      end
      tFrame._OnCompleteTransition = nil
      tFrame._OnCompleteEvent = nil
      if _nCinematicFrame then
        _NextCinematicFrame(tCinematic, fCallback, tCallbackArgs)
      else
        Debug.Printf("Residual OnComplete event...ignoring")
      end
    end
    
    local uGuid = GetActorGuid(tFrame.OnComplete)
    tFrame._OnCompleteEvent = Event.Create(Event.AnimationEvent, {
      uGuid,
      "CinematicEnd"
    }, _OnComplete, {1})
    tFrame._OnCompleteTransition = Event.Create(Event.HumanStateTransition, {
      uGuid,
      "*",
      "*",
      "complete"
    }, _OnComplete, {2})
  end
  if tFrame.Stall == true then
    Debug.Printf("Stalling frame...")
  end
  if not tFrame.OnComplete and not tFrame.OnTime and tFrame.Stall ~= true then
    Debug.Printf("No On-entry - continuing immediately")
    _NextCinematicFrame(tCinematic, fCallback, tCallbackArgs)
  end
end

_tNetSafeHardpoints = {
  "hp_camera_a",
  "hp_camera_b",
  "hp_menu_camera",
  "Bone_Attach_Root"
}
_tNetSafeShotNames = {
  "FaceToFace",
  "OverTheShoulderRight",
  "OverTheShoulderLeft",
  "CloseUp"
}

function GetHardpointIndex(sHardpointName)
  if not sHardpointName then
    return nil
  else
    local idx = 1
    for _, sHP in ipairs(_tNetSafeHardpoints) do
      if sHP == sHardpointName then
        return idx
      else
        idx = idx + 1
      end
    end
  end
  Debug.Printf("\tHardpoint - Could not find hardpoint " .. tostring(sHardpointName) .. " in _tNetSafeHardpoints")
  return nil
end

function GetShotNameIndex(sShotName)
  if not sShotName then
    return nil
  else
    local idx = 1
    for _, sName in ipairs(_tNetSafeShotNames) do
      if sName == sShotName then
        return idx
      else
        idx = idx + 1
      end
    end
  end
  Debug.Printf("\tShotName - Could not find shot name " .. tostring(sShotName) .. " in _tNetSafeShotNames")
  return nil
end

function _ProcessCameraSettings(tSettings)
  local netSafeSettings = {}
  netSafeSettings.nHold = 0
  if tSettings.bHold ~= nil then
    if tSettings.bHold == true then
      netSafeSettings.nHold = 1
    else
      netSafeSettings.nHold = 2
    end
  end
  netSafeSettings.nBlendTime = tSettings.nBlendTime
  netSafeSettings.tPosition = tSettings.tPosition
  if tSettings.sPositionObject then
    netSafeSettings.uPositionObject = GetActorGuid(tSettings.sPositionObject)
    if not netSafeSettings.uPositionObject then
      Debug.Printf("\tPosition - No guid corresponding to object " .. tostring(tSettings.sPositionObject))
    end
  end
  netSafeSettings.nPositionHardpoint = GetHardpointIndex(tSettings.sPositionHardpoint)
  netSafeSettings.tLookAt = tSettings.tLookAt
  netSafeSettings.bLookAtDirection = tSettings.bLookAtDirection
  if tSettings.sLookAtObject then
    netSafeSettings.uLookAtObject = GetActorGuid(tSettings.sLookAtObject)
    if not netSafeSettings.uLookAtObject then
      Debug.Printf("\tLookAt - No guid corresponding to object " .. tostring(tSettings.sLookAtObject))
    end
  end
  netSafeSettings.nLookAtHardpoint = GetHardpointIndex(tSettings.sLookAtHardpoint)
  if tSettings.tShot then
    netSafeSettings.tShot = {}
    netSafeSettings.tShot.nName = GetShotNameIndex(tSettings.tShot.sName)
    netSafeSettings.tShot.uBaseActor = GetActorGuid(tSettings.tShot.sBaseActor)
    netSafeSettings.tShot.uTargetActor = GetActorGuid(tSettings.tShot.sTargetActor)
  end
  NetSafeProcessCameraSettings(netSafeSettings)
end

function NetClientProcessCameraSettings(tSettings, tPosition, tLookAt, tShot)
  tSettings.tPosition = tPosition
  tSettings.tLookAt = tLookAt
  tSettings.tShot = tShot
  NetSafeProcessCameraSettings(tSettings)
end

function NetSafeProcessCameraSettings(tSettings)
  local uPrimaryPlayer = Player.GetPrimaryPlayer()
  local uSecondaryPlayer = Player.GetSecondaryPlayer()
  local uPrimaryCamera, uSecondaryCamera
  if uPrimaryPlayer then
    uPrimaryCamera = Player.GetCamera(uPrimaryPlayer)
  end
  if uSecondaryPlayer then
    uSecondaryCamera = Player.GetCamera(uSecondaryPlayer)
  end
  local tCameras = {uPrimaryCamera, uSecondaryCamera}
  if tSettings.nPositionHardpoint then
    tSettings.sPositionHardpoint = _tNetSafeHardpoints[tSettings.nPositionHardpoint]
  end
  if tSettings.nLookAtHardpoint then
    tSettings.sLookAtHardpoint = _tNetSafeHardpoints[tSettings.nLookAtHardpoint]
  end
  if tSettings.nHold ~= 0 then
    if tSettings.nHold == 1 then
      tSettings.bHold = true
    else
      tSettings.bHold = false
    end
  end
  for _, uCamera in ipairs(tCameras) do
    Debug.Printf("Camera Settings (" .. tostring(uCamera) .. "): ")
    if tSettings.bHold ~= nil then
      Debug.Printf("\tHold - " .. tostring(tSettings.bHold))
      Camera.Hold(uCamera, tSettings.bHold, tSettings.bHold, true)
    end
    if tSettings.nBlendTime then
      Camera.Blend(uCamera, tSettings.nBlendTime, true)
      Debug.Printf("\tBlendTime - " .. tSettings.BlendTime)
    end
    if tSettings.tPosition then
      Camera.SetPosition(uCamera, tSettings.tPosition[1], tSettings.tPosition[2], tSettings.tPosition[3], true)
      Debug.Printf("\tPosition - < " .. tSettings.tPosition[1] .. ", " .. tSettings.tPosition[2] .. ", " .. tSettings.tPosition[3] .. " >")
    elseif tSettings.uPositionObject then
      Camera.SetPosition(uCamera, tSettings.uPositionObject, tSettings.sPositionHardpoint, true)
      Debug.Printf("\tPosition - " .. tostring(tSettings.uPositionObject) .. "::" .. tostring(tSettings.sPositionHardpoint))
    end
    if tSettings.tLookAt then
      Camera.SetLookAt(uCamera, tSettings.tLookAt[1], tSettings.tLookAt[2], tSettings.tLookAt[3], tSettings.bLookAtDirection, true)
      Debug.Printf("\tLookAt - < " .. tSettings.tLookAt[1] .. ", " .. tSettings.tLookAt[2] .. ", " .. tSettings.tLookAt[3] .. " > ")
    elseif tSettings.uLookAtObject then
      Camera.SetLookAt(uCamera, tSettings.uLookAtObject, tSettings.sLookAtHardpoint, tSettings.bLookAtDirection, true)
      Debug.Printf("\tLookAt - " .. tostring(tSettings.uLookAtObject) .. "::" .. tostring(tSettings.sLookAtHardpoint))
    end
    if tSettings.tShot then
      if tSettings.tShot.nName then
        tSettings.tShot.sName = _tNetSafeShotNames[tSettings.tShot.nName]
      end
      Camera.SetShot(uCamera, tSettings.tShot.sName, tSettings.tShot.uBaseActor, tSettings.tShot.uTargetActor, true)
      Debug.Printf("\tShot - " .. tSettings.tShot.sName .. ", Base - " .. tostring(tSettings.tShot.uBaseActor) .. ", Target - " .. tostring(tSettings.tShot.uTargetActor))
    end
  end
end

function _CreateCheapCinematic(nType)
  local tCheapCinematic
  if nType == CHEAP_GREETING then
    local sGreeting, sAnim = _GetGreeting()
    if sGreeting then
      Debug.Printf("Greeting: " .. sGreeting)
      tCheapCinematic = {
        sParticipant1 = "Player1",
        sParticipant2 = "Starter",
        bFadeIn = true,
        tSequence = {
          {
            sSpeaker = "Starter",
            sCue = sGreeting,
            sAnim = sAnim
          }
        }
      }
    else
      return nil
    end
  elseif nType == CHEAP_SPECIALCASEGREETING then
    local sVo
    if Net.IsClient() then
      sVo = WifPmcInterior.NetSafeGetSpecialCaseGreeting()
    else
      sVo = _oStarter:GetSpecialCaseGreeting()
    end
    if sVo then
      tCheapCinematic = {
        sParticipant1 = "Player1",
        sParticipant2 = "Starter",
        bFadeIn = true,
        tSequence = {
          {sSpeaker = "Starter", sCue = sVo}
        }
      }
    else
      return nil
    end
  elseif nType == CHEAP_STARTINTRO then
  elseif nType == CHEAP_JOBREQUEST then
    local sJobRequest = _GetJobRequest()
    if sJobRequest then
      Debug.Printf("Job request: " .. sJobRequest)
      local tGreetingAnims = {
        Chris = "player_chris_job_briefing_greeting",
        Jennifer = "player_jennifer_job_briefing_greeting",
        Mattias = "player_mattias_job_briefing_greeting_fb"
      }
      tCheapCinematic = {
        sParticipant1 = "Player1",
        sParticipant2 = "Starter",
        tSequence = {
          {
            sSpeaker = "Player1",
            sCue = sJobRequest,
            sAnim = tGreetingAnims[MrxUtil.GetPrimaryCharacterName()]
          }
        }
      }
    else
      return nil
    end
  elseif nType == CHEAP_JOBACCEPT then
    local sCue = _GetSpielResponse(true)
    if sCue then
      local tYesAnims = {
        Chris = "player_chris_job_briefing_yes",
        Jennifer = "player_jennifer_job_briefing_yes",
        Mattias = "player_mattias_job_briefing_yes_fb"
      }
      local sAnim = tYesAnims[MrxUtil.GetPrimaryCharacterName()]
      local tSequence = {
        {
          sSpeaker = "Player1",
          sCue = sCue,
          sAnim = sAnim
        }
      }
      tCheapCinematic = {
        sParticipant1 = "Player1",
        sParticipant2 = "Starter",
        tSequence = tSequence
      }
    else
      return nil
    end
  elseif nType == CHEAP_JOBDECLINE then
    local sCue = _GetSpielResponse(false)
    if sCue then
      local tNoAnims = {
        Chris = "player_chris_job_briefing_no",
        Jennifer = "player_jennifer_job_briefing_no",
        Mattias = "player_mattias_job_briefing_no_fb"
      }
      local sAnim = tNoAnims[MrxUtil.GetPrimaryCharacterName()]
      local tSequence = {
        {
          sSpeaker = "Player1",
          sCue = sCue,
          sAnim = sAnim
        }
      }
      tCheapCinematic = {
        sParticipant1 = "Player1",
        sParticipant2 = "Starter",
        tSequence = tSequence
      }
    else
      return nil
    end
  elseif nType == CHEAP_WAGERBEGINWIN then
    local tStarterVo = {
      Eva = {
        "Eva.Wager.Lose01",
        "Eva.Wager.Lose02"
      },
      Ewan = {
        "Ewan.Wager.Lose01",
        "Ewan.Wager.Lose02"
      },
      Fiona = {
        "Fiona.Wager.Lose01",
        "Fiona.Wager.Lose02"
      },
      Misha = {
        "Misha.Wager.Lose01",
        "Misha.Wager.Lose02"
      }
    }
    local sStarterName = _oStarter:GetPmcName()
    local sStarterWagerVo = MrxUtil.GetRandomTableElement(tStarterVo[sStarterName])
    local sStarterWagerAnim = _GetGenericTalkBodyAnim("Starter")
    tCheapCinematic = {
      sParticipant1 = "Player1",
      sParticipant2 = "Starter",
      bFadeIn = true,
      tSequence = {
        {
          sSpeaker = "Starter",
          sCue = sStarterWagerVo,
          sAnim = sStarterWagerAnim
        }
      }
    }
  elseif nType == CHEAP_WAGERBEGINLOSE then
    local tStarterVo = {
      Eva = {
        "Eva.Wager.Win01",
        "Eva.Wager.Win02"
      },
      Ewan = {
        "Ewan.Wager.Win01",
        "Ewan.Wager.Win02"
      },
      Fiona = {
        "Fiona.Wager.Win01",
        "Fiona.Wager.Win02"
      },
      Misha = {
        "Misha.Wager.Win01",
        "Misha.Wager.Win02"
      }
    }
    local sStarterName = _oStarter:GetPmcName()
    local sStarterWagerVo = MrxUtil.GetRandomTableElement(tStarterVo[sStarterName])
    local sStarterWagerAnim = _GetGenericTalkBodyAnim("Starter")
    tCheapCinematic = {
      sParticipant1 = "Player1",
      sParticipant2 = "Starter",
      bFadeIn = true,
      tSequence = {
        {
          sSpeaker = "Starter",
          sCue = sStarterWagerVo,
          sAnim = sStarterWagerAnim
        }
      }
    }
  elseif nType == CHEAP_WAGERWON then
    local tVo = {
      Chris = {
        "Chris.Wager.Won.01",
        "Chris.Wager.Won.02"
      },
      Jennifer = {
        "Jen.Wager.Won.01",
        "Jen.Wager.Won.02"
      },
      Mattias = {
        "Mattias.Wager.Won.01",
        "Mattias.Wager.Won.02"
      }
    }
    local sHeroName = MrxUtil.GetPrimaryCharacterName()
    local sPlayerWagerVo = MrxUtil.GetRandomTableElement(tVo[sHeroName])
    local sPlayerWagerAnim = _GetGenericTalkBodyAnim("Player1")
    tCheapCinematic = {
      sParticipant1 = "Player1",
      sParticipant2 = "Starter",
      tSequence = {
        {
          sSpeaker = "Player1",
          sCue = sPlayerWagerVo,
          sAnim = sPlayerWagerAnim
        }
      }
    }
  elseif nType == CHEAP_WAGERLOST then
    local tVo = {
      Chris = {
        "Chris.Wager.Lost.01",
        "Chris.Wager.Lost.02"
      },
      Jennifer = {
        "Jen.Wager.Lost.01",
        "Jen.Wager.Lost.02"
      },
      Mattias = {
        "Mattias.Wager.Lost.01",
        "Mattias.Wager.Lost.02"
      }
    }
    local sHeroName = MrxUtil.GetPrimaryCharacterName()
    local sPlayerWagerVo = MrxUtil.GetRandomTableElement(tVo[sHeroName])
    local sPlayerWagerAnim = _GetGenericTalkBodyAnim("Player1")
    tCheapCinematic = {
      sParticipant1 = "Player1",
      sParticipant2 = "Starter",
      tSequence = {
        {
          sSpeaker = "Player1",
          sCue = sPlayerWagerVo,
          sAnim = sPlayerWagerAnim
        }
      }
    }
  elseif nType == CHEAP_WAGERCHICKENSUIT then
    local tVo = {
      Chris = {
        "Chris.CustomOutfit.Chicken.01",
        "Chris.CustomOutfit.Chicken.02",
        "Chris.CustomOutfit.Chicken.03",
        "Chris.CustomOutfit.Chicken.04",
        "Chris.CustomOutfit.Chicken.05"
      },
      Jennifer = {
        "Jen.CustomOutfit.Chicken.01",
        "Jen.CustomOutfit.Chicken.02",
        "Jen.CustomOutfit.Chicken.03",
        "Jen.CustomOutfit.Chicken.04",
        "Jen.CustomOutfit.Chicken.05"
      },
      Mattias = {
        "Mattias.CustomOutfit.Chicken.01",
        "Mattias.CustomOutfit.Chicken.02",
        "Mattias.CustomOutfit.Chicken.03",
        "Mattias.CustomOutfit.Chicken.04",
        "Mattias.CustomOutfit.Chicken.05"
      }
    }
    local sHeroName = MrxUtil.GetPrimaryCharacterName()
    local sPlayerWagerVo = MrxUtil.GetRandomTableElement(tVo[sHeroName])
    local sPlayerWagerAnim
    if MrxUtil.GetPrimaryCharacterName() == "Jennifer" then
      sPlayerWagerAnim = "all_starter03_job_briefing_spiel"
    else
      sPlayerWagerAnim = "all_starter02_job_briefing_spiel"
    end
    tCheapCinematic = {
      sParticipant1 = "Player1",
      sParticipant2 = "Starter",
      tSequence = {
        {
          sSpeaker = "Player1",
          sCue = sPlayerWagerVo,
          sAnim = sPlayerWagerAnim
        }
      }
    }
  elseif nType == CHEAP_HINT then
    local sCue = WifHints.GetHint(_oStarter:GetPmcName())
    tCheapCinematic = {
      sParticipant1 = "Player1",
      sParticipant2 = "Starter",
      tSequence = {
        {sSpeaker = "Starter", sCue = sCue}
      }
    }
  elseif nType == CHEAP_GOODBYE then
    local sGoodbye = _GetGoodbye()
    if not _oStarter:IsPmcStarter() and sGoodbye then
      Debug.Printf("Goodbye: " .. sGoodbye)
      local tSequence = {
        {sSpeaker = "Starter", sCue = sGoodbye}
      }
      if _oStarter:IsMale() then
        tSequence[1].sAnim = "all_starter02_job_briefing_goodbye"
      else
        tSequence[1].sAnim = "all_starter03_job_briefing_goodbye"
      end
      tCheapCinematic = {
        sParticipant1 = "Player1",
        sParticipant2 = "Starter",
        tSequence = tSequence
      }
    else
      return nil
    end
  elseif nType == CHEAP_PMCWAGER then
    local tStarterVo = {
      Eva = {
        "Eva.Wager.OfferGeneric01",
        "Eva.Wager.OfferGeneric02"
      },
      Ewan = {
        "Ewan.Wager.OfferGeneric01",
        "Ewan.Wager.OfferGeneric02"
      },
      Fiona = {
        "Fiona.Wager.OfferGeneric01"
      },
      Misha = {
        "Misha.Wager.OfferGeneric01"
      }
    }
    local sStarterName = _oStarter:GetPmcName()
    local sStarterWagerVo = MrxUtil.GetRandomTableElement(tStarterVo[sStarterName])
    local sStarterWagerAnim = _GetGenericTalkBodyAnim("Starter")
    tCheapCinematic = {
      sParticipant1 = "Player1",
      sParticipant2 = "Starter",
      tSequence = {
        {
          sSpeaker = "Starter",
          sCue = sStarterWagerVo,
          sAnim = sStarterWagerAnim
        }
      }
    }
  end
  if Net.IsServer() and tCheapCinematic then
    Net.SetBriefingCheapCinematic(nType)
  end
  return tCheapCinematic
end

function _ProcessCheapCinematic(tData, fCallback, tCallbackArgs)
  Debug.Printf("_ProcessCheapCinematic")
  local sParticipant1 = tData.sParticipant1
  local sParticipant2 = tData.sParticipant2
  Debug.Printf("- sParticipant1 = " .. tostring(sParticipant1))
  Debug.Printf("- sParticipant2 = " .. tostring(sParticipant2))
  local bFadeIn = tData.bFadeIn
  tData.bFadeIn = nil
  _tAnimationEvents = {}
  
  local function _SwitchCamera(sSpeakerName)
    local bSpeakerIsPartcipant1 = sSpeakerName == sParticipant1
    local bSpeakerIsPartcipant2 = sSpeakerName == sParticipant2
    if not bSpeakerIsPartcipant1 and not bSpeakerIsPartcipant2 then
      return false
    end
    local tSettings
    if bSpeakerIsPartcipant1 and tData.tParticipant1CamOverride then
      tSettings = tData.tParticipant1CamOverride
    elseif bSpeakerIsPartcipant2 and tData.tParticipant2CamOverride then
      tSettings = tData.tParticipant2CamOverride
    else
      local sLookAtObject = sParticipant1
      local sPositionObject = sParticipant2
      local sCameraNickname = "OverTheShoulderLeft"
      if bSpeakerIsPartcipant2 then
        sLookAtObject = sParticipant2
        sPositionObject = sParticipant1
        sCameraNickname = "OverTheShoulderRight"
      end
      tSettings = {
        tShot = {
          sName = sCameraNickname,
          sBaseActor = sPositionObject,
          sTargetActor = sLookAtObject
        }
      }
    end
    _ProcessCameraSettings(tSettings)
    return true
  end
  
  local function _StopAnimation(sSpeakerName)
    Debug.Printf("- _GetGenericIdleBodyAnim .. " .. tostring(sSpeakerName))
    local sIdleAnim = _GetGenericIdleBodyAnim(sSpeakerName)
    Debug.Printf("- sAnimName = " .. tostring(sIdleAnim))
    local uGuid = GetActorGuid(sSpeakerName)
    if sIdleAnim and uGuid then
      Human.PlayRawAnimation(uGuid, sIdleAnim, true, true, 0.5, false, true)
    end
    if _tAnimationEvents and _tAnimationEvents[sSpeakerName] then
      Event.Delete(_tAnimationEvents[sSpeakerName])
      _tAnimationEvents[sSpeakerName] = nil
    end
  end
  
  local function _PlayAnimation(sSpeakerName, sAnimation, bLoop)
    local uGuid = GetActorGuid(sSpeakerName)
    if not uGuid or not sAnimation then
      return
    end
    local bSuccess = Human.PlayRawAnimation(uGuid, sAnimation, bLoop, true, 0.5, false, true)
    if not bLoop then
      _tAnimationEvents[sSpeakerName] = Event.Create(Event.HumanAnimationNearlyCompleted, {uGuid, 0.1}, _StopAnimation, {sSpeakerName})
    end
  end
  
  local function _PlayFlash(sFlashFile, nTime)
    _tFlashTimers = _tFlashTimers or {}
    local iTimer = table.getn(_tFlashTimers)
    _ShowFlashObject(sFlashFile)
    _tFlashTimers[iTimer] = Event.Create(Event.TimerRelative, {nTime}, _RemoveFlashObject, {sFlashFile, iTimer})
  end
  
  local sCharacterName = MrxUtil.GetPrimaryCharacterName()
  local tVoSequenceData = {}
  
  local function _YetAnotherLoadingFunction()
    local function _PlayCinematic()
      MrxVoSequence.Start(tVoSequenceData, true, MrxVoSequence.knPriorityBriefing, false)
      
      _CreateSkipEvent(fCallback, tCallbackArgs)
    end
    
    if bFadeIn then
      _Fade(true, _PlayCinematic)
    else
      _PlayCinematic()
    end
  end
  
  MrxUtil.SetupLoadingCallback(_THIS, _YetAnotherLoadingFunction)
  _nLoadPending = _nLoadPending + 1
  for i, vStage in ipairs(tData.tSequence) do
    if type(vStage) == "table" then
      if vStage.sSpeaker and vStage.sCue then
        local sSpeakerName = vStage.sSpeaker
        local sCueName = vStage.sCue
        local sAnimName = vStage.sAnim
        Debug.Printf("- sSpeakerName = " .. tostring(sSpeakerName))
        if sSpeakerName == "Player1" then
          if type(sCueName) == "table" then
            sCueName = sCueName[sCharacterName]
          end
          if type(sAnimName) == "table" then
            sAnimName = sAnimName[sCharacterName]
          end
        end
        Debug.Printf("- sCueName = " .. tostring(sCueName))
        Debug.Printf("- sAnimName = " .. tostring(sAnimName))
        if i == 1 then
          if bFadeIn then
            _SwitchCamera(sSpeakerName)
          else
            table.insert(tVoSequenceData, {
              _SwitchCamera,
              {sSpeakerName},
              true
            })
          end
        end
        if sAnimName == nil or sAnimName == "" then
          Debug.Printf("- _GetGenericTalkBodyAnim .. " .. tostring(sSpeakerName))
          sAnimName = _GetGenericTalkBodyAnim(sSpeakerName)
          Debug.Printf("- sAnimName = " .. tostring(sAnimName))
        end
        table.insert(tVoSequenceData, {
          _PlayAnimation,
          {
            sSpeakerName,
            sAnimName,
            true
          },
          true
        })
        table.insert(tVoSequenceData, 0.1)
        table.insert(tVoSequenceData, {
          sCueName,
          GetActorGuid(sSpeakerName)
        })
        local tNextVo
        local j = i + 1
        while not tNextVo do
          local vNextStep = tData.tSequence[j]
          if vNextStep == nil then
            break
          elseif type(vNextStep) == "table" then
            tNextVo = vNextStep
          end
          j = j + 1
        end
        local sNextSpeaker
        if tNextVo then
          sNextSpeaker = tNextVo.sSpeaker
        end
        if sSpeakerName ~= sNextSpeaker then
          Debug.Printf("- Setting up a camera switch")
          table.insert(tVoSequenceData, {
            _StopAnimation,
            {sSpeakerName},
            true
          })
          if sNextSpeaker then
            table.insert(tVoSequenceData, {
              _SwitchCamera,
              {sNextSpeaker},
              true
            })
            table.insert(tVoSequenceData, 0.1)
          end
        else
          Debug.Printf("- NOT Setting up a camera switch")
        end
      elseif vStage.sFlashFile and vStage.nTime then
        _nLoadPending = _nLoadPending + 1
        _AddFlashObject(vStage.sFlashFile, nil, MrxUtil.LoadingCallback, {_THIS})
        table.insert(tVoSequenceData, {
          _PlayFlash,
          {
            vStage.sFlashFile,
            vStage.nTime
          },
          true
        })
      end
    elseif type(vStage) == "number" then
      table.insert(tVoSequenceData, vStage)
    end
  end
  if fCallback then
    table.insert(tVoSequenceData, {
      fCallback,
      tCallbackArgs,
      true
    })
  end
  _StopAnimation(sParticipant1)
  _StopAnimation(sParticipant2)
  MrxUtil.LoadingCallback(_THIS)
end

function _StopCheapCinematic()
  if Net.IsServer() then
    Net.SetBriefingCheapCinematic(0)
  end
  MrxVoSequence.Stop()
  if _tAnimationEvents then
    for sSpeaker, uEvent in pairs(_tAnimationEvents) do
      Event.Delete(uEvent)
    end
    _tAnimationEvents = nil
  end
end

function _ProcessCheapCinematicAsText(tData, fCallback, tCallbackArgs)
  _ProcessCameraSettings({
    tShot = {
      sName = "CloseUp",
      sBaseActor = "Player1",
      sTargetActor = "Starter"
    }
  })
  local sMessageText = ""
  local nStages = #tData.tSequence
  for i, vStage in ipairs(tData.tSequence) do
    if type(vStage) == "table" and vStage.sSpeaker and vStage.sCue then
      local sSpeakerName = vStage.sSpeaker
      local sCueName = vStage.sCue
      if sSpeakerName == "Player1" then
        local uSpeaker = GetActorGuid(sSpeakerName)
        if uSpeaker then
          sSpeakerName = Object.GetLocalizedName(uSpeaker) or sSpeakerName
        end
      else
        sSpeakerName = sSpeakerName == "Starter" and _oStarter:GetPlayerVisibleName() or sSpeakerName
      end
      sMessageText = sMessageText .. sSpeakerName .. ": [" .. sCueName .. "]"
      if i < nStages then
        sMessageText = sMessageText .. [[


]]
      end
    end
  end
  MrxGui.DisplayDialogBox(Player.GetPrimaryPlayer(), sMessageText, {
    "[Generic.Continue]"
  }, 1, fCallback, tCallbackArgs or {}, 0, 0, "center", "center", false)
end

function _CleanupCinematic(tCinematic)
  local sCharName = MrxUtil.GetPrimaryCharacterName()
  _ClearAllFlashObjects()
  _SetCameraEffects(0, "RestoreAll", 1)
  if tCinematic.tCameraEffects then
    local tCameraEffects = tCinematic.tCameraEffects[sCharName]
    for i, tCameraEffect in ipairs(tCameraEffects) do
      if tCameraEffect._CameraEffectsTimer then
        Event.Delete(tCameraEffect._CameraEffectsTimer)
        tCameraEffect._CameraEffectsTimer = nil
      end
    end
  end
  local tCharCinematic = tCinematic[sCharName]
  if tCharCinematic then
    for i, tFrame in ipairs(tCharCinematic) do
      if tFrame._OnTimeEvent then
        Event.Delete(tFrame._OnTimeEvent)
        tFrame._OnTimeEvent = nil
      end
      if tFrame._OnCompleteEvent then
        Event.Delete(tFrame._OnCompleteEvent)
        tFrame._OnCompleteEvent = nil
      end
      if tFrame._OnCompleteTransition then
        Event.Delete(tFrame._OnCompleteTransition)
        tFrame._OnCompleteTransition = nil
      end
    end
  end
end

function _StopClientCheapCinematic()
  Debug.Printf("_StopClientCheapCinematic")
  _StopCheapCinematic()
  _DeleteSkipEvent()
  _ProcessCameraSettings({bHold = false})
  _nCinematicFrame = nil
  _nSlideFrame = nil
  _tSlide = nil
  Event.Create(Event.TimerRelative, {0.13}, _ProcessCameraSettings, {
    {
      tShot = {
        sName = "OverTheShoulderLeft",
        sBaseActor = "Starter",
        sTargetActor = "Player1"
      }
    }
  })
end

function _CinematicComplete()
  Debug.Printf("Cinematic - All Complete")
  local tConfig = _GetSelectedBriefingConfig()
  if tConfig.tCinematic then
    VO.CancelAll()
  elseif tConfig.tCheapCinematic then
    _StopCheapCinematic()
  end
  _DeleteSkipEvent()
  _ProcessCameraSettings({bHold = false})
  _nCinematicFrame = nil
  _nSlideFrame = nil
  _tSlide = nil
  _SetActorsToDefaultPose({"Player1", "Starter"}, tConfig.tCinematic == nil)
  if tConfig.tCinematic then
    _CleanupCinematic(tConfig.tCinematic)
  else
    _ClearAllFlashObjects()
    local tSettings
    if tConfig.tCheapCinematic then
      if tConfig.tCheapCinematic.sParticipant1 == "Player1" then
        tSettings = tConfig.tCheapCinematic.tParticipant1CamOverride
      elseif tConfig.tCheapCinematic.sParticipant2 == "Player1" then
        tSettings = tConfig.tCheapCinematic.tParticipant2CamOverride
      end
    end
    tSettings = tSettings or {
      tShot = {
        sName = "OverTheShoulderLeft",
        sBaseActor = "Starter",
        sTargetActor = "Player1"
      }
    }
    Event.Create(Event.TimerRelative, {0.13}, _ProcessCameraSettings, {tSettings})
  end
  _DisplayConfirmDialog()
end

function _ShowFlashObject(sName)
  if not _tFlashObjects then
    return
  end
  local oFlash = _tFlashObjects[sName]
  if not oFlash then
    return
  end
  Debug.Printf("Cinematic - Showing Flash " .. sName)
  oFlash:SetVisible(true)
  oFlash:Play()
end

function _AddFlashObject(sName, tPosition, fCallback, tCallbackData)
  if not _tFlashObjects then
    _tFlashObjects = {}
  elseif _tFlashObjects[sName] then
    MrxUtil.CallWithOptionalArgs(fCallback, tCallbackData)
    return
  end
  Debug.Printf("Cinematic - Adding Flash " .. sName)
  local oFlash = MrxGui.FlashWidget:new()
  oFlash:SetIgnoresPause(false)
  if tPosition then
    oFlash:SetPosition(tPosition[1], tPosition[2], tPosition[3], tPosition[4])
  else
    oFlash:SetFullscreen(true)
  end
  oFlash:SetSwfFile(sName, fCallback, tCallbackData)
  oFlash:SetOwner(Player.GetLocalPlayer())
  oFlash:SetVisible(false)
  oFlash:Pause()
  MrxGui.AddWidget(oFlash)
  _tFlashObjects[sName] = oFlash
end

function _RemoveFlashObject(sName, iTimer)
  if not _tFlashObjects then
    return
  end
  local oFlash = _tFlashObjects[sName]
  if not oFlash then
    return
  end
  Debug.Printf("Cinematic - Removing Flash " .. sName)
  _tFlashObjects[sName] = nil
  if iTimer then
    _tFlashTimers[iTimer] = nil
  end
  oFlash:SetSwfFile(nil)
  MrxGui.RemoveWidget(oFlash)
  oFlash:Delete()
end

function _ClearAllFlashObjects()
  if not _tFlashObjects then
    return
  end
  Debug.Printf("Cinematic - Clearing all flash objects")
  for sName, oFlash in pairs(_tFlashObjects) do
    _RemoveFlashObject(sName)
  end
  if _tFlashTimers then
    for _, uTimerEvent in pairs(_tFlashTimers) do
      Event.Delete(uTimerEvent)
    end
    _tFlashTimers = nil
  end
  _tFlashObjects = nil
end

function _SetupPlayers(bOn)
  local tPlayers = Player.GetAllPlayers()
  for _, uPlayer in ipairs(tPlayers) do
    if bOn then
      Debug.Printf("Cinematic - Enabling Player " .. tostring(uPlayer))
    else
      Debug.Printf("Cinematic - Disabling Player " .. tostring(uPlayer))
    end
    Player.SetCinematicMode(uPlayer, not bOn, "Bone_Attach_Root", 0, true)
    local uChar = Player.GetCharacter(uPlayer)
    if Human.IsCarrying(uChar) then
      Human.Drop(uChar)
    end
    Human.Scrub(uChar)
    if bOn and not Vehicle.GetFromRider(uChar) then
      Human.SetState(uChar, "Upright", "Idle")
    end
    Human.SetJostleEnabled(uChar, bOn)
  end
end

function _BindFaceAnim(bBind, sActor, sFaceFile)
  if not sFaceFile then
    if sActor == "Player1" then
      sFaceFile = "Global_Job_Briefing_" .. MrxUtil.GetPrimaryCharacterName()
    else
      sFaceFile = _oStarter:GetGlobalFaceFxSet()
      if not sFaceFile then
        Debug.Printf("@!! No global FaceFx for actor " .. sActor)
        return false
      end
    end
  end
  local fBind
  if bBind then
    fBind = Animation.BindFaceAnimSet
    Debug.Printf("@ Binding FaceFx " .. sFaceFile .. " to " .. sActor)
  else
    fBind = Animation.UnbindFaceAnimSet
    Debug.Printf("@ Unbinding FaceFx " .. sFaceFile .. " to " .. sActor)
  end
  local uGuid = GetActorGuid(sActor)
  local bSuccess = fBind(uGuid, sFaceFile)
  if not bSuccess then
    Debug.Printf("@!! FaceFx operation failed!")
  end
  return bSuccess
end

function _AttachActorsToHardpoints(tActorsToHardpoints)
  local uObject = GetActorGuid("HqInterior")
  if not uObject then
    Debug.Printf("Failed to find HqInterior, canceling actor setup")
    return
  end
  for sName, sHardpoint in pairs(tActorsToHardpoints) do
    local uGuid = GetActorGuid(sName)
    Debug.Printf("Attaching actor " .. sName .. "  to hardpoint " .. sHardpoint)
    Object.DisablePhysics(uGuid)
    Object.Attach(uObject, sHardpoint, uGuid)
    Object.SetTransformToObject(uGuid, uObject, sHardpoint)
    Human.PersistTransform(uGuid)
  end
end

function _DetachActorsFromHardpoints(tActors)
  local uObject = GetActorGuid("HqInterior")
  if not uObject then
    Debug.Printf("Failed to find HqInterior, canceling actor setup")
    return
  end
  for _, sName in ipairs(tActors) do
    local uGuid = GetActorGuid(sName)
    Debug.Printf("Detaching actor " .. sName .. " ( " .. tostring(uGuid) .. " ) from hardpoint")
    Object.Detach(uObject, uGuid)
    Object.EnablePhysics(uGuid)
  end
end

function _AttachActorsToLocations(tActorsToLocations)
  local uObject = Pg.GetGuidByName("HqInterior")
  for sName, sLocation in pairs(tActorsToLocations) do
    local uGuid = GetActorGuid(sName)
    Debug.Printf("Attaching actor " .. sName .. "  to location " .. sLocation)
    Object.Detach(uObject, uGuid)
    Object.SetTransformToObject(uGuid, Pg.GetGuidByName(sLocation))
    Human.PersistTransform(uGuid)
    Object.DisablePhysics(uGuid)
  end
end

function _AttachActorsToStartingLocations()
  if _oStarter:IsPmcStarter() then
    local sStarterId = _oStarter:GetName()
    local tBriefingLocs = WifPmcInterior.GetStarterBriefingLocs(sStarterId)
    _AttachActorsToLocations({
      Player1 = tBriefingLocs[2],
      Starter = tBriefingLocs[1]
    })
  else
    _AttachActorsToHardpoints({Player1 = "hp_playerA", Starter = "hp_starter"})
  end
end

function _SaveActorsOriginalPositions(tActors)
  if not _tOriginalActorPositions then
    _tOriginalActorPositions = {}
  end
  for _, sName in ipairs(tActors) do
    local uGuid = GetActorGuid(sName)
    local x, y, z = Object.GetPosition(uGuid)
    local yaw = Object.GetYaw(uGuid)
    _tOriginalActorPositions[uGuid] = {
      x,
      y,
      z,
      yaw
    }
  end
end

function _RestoreActorsToOriginalPositions(tActors)
  if not _tOriginalActorPositions then
    return
  end
  for _, sName in ipairs(tActors) do
    local uGuid = GetActorGuid(sName)
    if _tOriginalActorPositions[uGuid] then
      local x, y, z, yaw = unpack(_tOriginalActorPositions[uGuid])
      Object.SetPosition(uGuid, x, y, z)
      Object.SetYaw(uGuid, yaw)
      Object.EnablePhysics(uGuid)
    end
  end
  _tOriginalActorPositions = nil
end

function _SetActorsToDefaultPose(tActors, bBlend)
  local nBlendTime = 0.5
  if bBlend == false then
    nBlendTime = -1
  end
  for _, sName in ipairs(tActors) do
    local uGuid = GetActorGuid(sName)
    local sAnim = _GetGenericIdleBodyAnim(sName)
    Human.PlayRawAnimation(uGuid, sAnim, true, bBlend, nBlendTime, false, true)
  end
end

function GetActorGuid(sActor)
  if sActor == "Player1" then
    return Player.GetPrimaryCharacter()
  elseif sActor == "Player2" then
    return Player.GetSecondaryCharacter()
  elseif sActor == "Starter" then
    return _oStarter:GetActor()
  end
  return Pg.GetGuidByName(sActor)
end

function GetSpielFileName(sMissionName)
  local sCharName = MrxUtil.GetPrimaryCharacterName()
  if sMissionName == "ChiCon009" then
    return "Spiel_Job_Chi09_" .. sCharName
  elseif sMissionName == "OilCon020" then
    return "Spiel_Job_Oil00_" .. sCharName
  elseif sMissionName == "OilCon050" then
    return "Spiel_Job_Oil01_" .. sCharName
  end
  local sFaction, bContract, nNumber = MrxUtil.ExplodeMissionName(sMissionName)
  local sMissionType = "Job"
  if bContract then
    sMissionType = "MinorContract"
  end
  return "Spiel_" .. sMissionType .. "_" .. sFaction .. string.format("%02d", nNumber) .. "_" .. sCharName
end

function GetAnimSet(vSet, sActor)
  local sSet = vSet
  if sActor == "Player1" then
    local sPrimaryCharacter = MrxUtil.GetPrimaryCharacterName()
    if type(vSet) == "table" and vSet[sPrimaryCharacter] then
      sSet = vSet[sPrimaryCharacter]
      ASSERT(type(sSet) == "string")
    else
      sSet = nil
    end
  end
  return sSet
end

function _GetGreeting()
  local sAttitude = MrxFactionManager.GetAttitudeLabel(_oStarter:GetFaction(), "Pmc")
  local tMappings = {Neutral = "Neutral", Friendly = "Positive"}
  local sAttitude = tMappings[sAttitude]
  if not sAttitude then
    return
  end
  if not _tBriefingWrapper then
    return
  end
  local tGreetings = _tBriefingWrapper.Greetings
  if not tGreetings then
    return
  end
  local bInitial = not _oStarter:HasCardBeenDisplayed()
  if bInitial then
    tGreetings = tGreetings.Initial
  else
    tGreetings = tGreetings.Subsequent
  end
  if not tGreetings then
    return
  end
  tGreetings = tGreetings[sAttitude]
  if not tGreetings then
    return
  end
  local sGreeting
  if bInitial then
    sGreeting = tGreetings[MrxUtil.GetPrimaryCharacterName()]
  else
    sGreeting = MrxUtil.GetRandomTableElement(tGreetings)
  end
  local sAnim
  if _oStarter:IsMale() then
    if sAttitude == "Negative" then
      sAnim = "all_starter02_job_briefing_greeting_angry"
    elseif sAttitude == "Neutral" then
      sAnim = "all_starter02_job_briefing_greeting_neutral"
    elseif sAttitude == "Positive" then
      sAnim = "all_starter02_job_briefing_greeting_happy"
    end
  elseif sAttitude == "Negative" then
    sAnim = "all_starter03_job_briefing_greeting_angry"
  elseif sAttitude == "Neutral" then
    sAnim = "all_starter03_job_briefing_greeting_neutral"
  elseif sAttitude == "Positive" then
    sAnim = "all_starter03_job_briefing_greeting_happy"
  end
  return sGreeting, sAnim
end

function _GetJobRequest()
  local tCues = _tHeroWrapperVo.tJobRequest[MrxUtil.GetPrimaryCharacterName()]
  return MrxUtil.GetRandomTableElement(tCues)
end

function _GetSpielResponse(bAccepted)
  local sTableName
  if bAccepted then
    sTableName = "tJobAccept"
  else
    sTableName = "tJobDecline"
  end
  local tCues = _tHeroWrapperVo[sTableName][MrxUtil.GetPrimaryCharacterName()]
  return MrxUtil.GetRandomTableElement(tCues)
end

function _GetGoodbye()
  if not _tBriefingWrapper then
    return
  end
  local tCues = _tBriefingWrapper.Goodbyes
  if tCues then
    return MrxUtil.GetRandomTableElement(tCues)
  end
end

function _GetGenericTalkBodyAnim(sSpeakerName)
  local t = {}
  if sSpeakerName == "Starter" then
    if _oStarter:IsMale() then
      t[1] = "all_starter02_job_briefing_spiel"
    else
      t[1] = "all_starter03_job_briefing_spiel"
    end
  else
    local sCharacterName = MrxUtil.GetPrimaryCharacterName()
    local tTalkAnims = {
      Chris = "player_chris_job_briefing_spiel",
      Jennifer = "player_jennifer_job_briefing_spiel",
      Mattias = "player_mattias_job_briefing_spiel_fb"
    }
    t[1] = tTalkAnims[sCharacterName]
  end
  return MrxUtil.GetRandomTableElement(t)
end

function _GetGenericIdleBodyAnim(sSpeakerName)
  local t = {}
  if sSpeakerName == "Starter" then
    if _oStarter:IsMale() then
      t[1] = "all_starter02_job_briefing_idle"
    else
      t[1] = "all_starter03_job_briefing_idle"
    end
  else
    local sCharacterName = MrxUtil.GetPrimaryCharacterName()
    local tIdleAnims = {
      Chris = "player_chris_job_briefing_idle",
      Jennifer = "player_jennifer_job_briefing_idle",
      Mattias = "player_mattias_job_briefing_idle_fb"
    }
    t[1] = tIdleAnims[sCharacterName]
  end
  return MrxUtil.GetRandomTableElement(t)
end

function _GetSelectedBriefingConfig()
  if not (_tBriefings and _sSelectedMission) or not _tBriefings[_sSelectedMission] then
    return nil
  end
  return _tBriefings[_sSelectedMission].tConfig
end

function _CreateSkipEvent(fCallback, tCallbackArgs)
  if Net.IsClient() then
    return
  end
  local sButton = "selection"
  if Sys.IsConfirmOnCircle() then
    sButton = "cancel"
  end
  Debug.Printf("_CreateSkipEvent")
  Debug.Printf(Debug.GetCallstack())
  
  local function _Go()
    _uButtonEvent = Event.CreatePersistent(Event.Button, {
      Player.GetLocalPlayer(),
      sButton,
      "press",
      true
    }, function()
      Hud.SubtitleBuffer:Clear({})
      MrxUtil.CallWithOptionalArgs(fCallback, tCallbackArgs)
    end)
  end
  
  _uButtonDelayEvent = Event.Create(Event.TimerRelative, {0.5}, _Go)
end

function _DeleteSkipEvent()
  Debug.Printf("_DeleteSkipEvent")
  Debug.Printf(Debug.GetCallstack())
  if _uButtonDelayEvent then
    Event.Delete(_uButtonDelayEvent)
    _uButtonDelayEvent = nil
  end
  if _uButtonEvent then
    Event.Delete(_uButtonEvent)
    _uButtonEvent = nil
  end
end

function _ProcessCameraEffects(tData, tCameraEffects)
  for k, tCameraEffect in pairs(tCameraEffects) do
    tCameraEffect._CameraEffectsTimer = Event.Create(Event.TimerRelative, {
      tCameraEffect.nTime,
      false
    }, function()
      if type(tCameraEffect.tDepthOfField) == "table" then
        _SetCameraEffects(0, "SetDOF", 0, 0, tCameraEffect.tDepthOfField[4] - 0.5, tCameraEffect.tDepthOfField[4] - 0.1, tCameraEffect.tDepthOfField[5] + 0.2, tCameraEffect.tDepthOfField[5] + 5, tCameraEffect.tDepthOfField[9] or 1)
      end
      if type(tCameraEffect.tFieldOfView) == "table" then
        _SetCameraEffects(0, "SetFOV", 0, tCameraEffect.tFieldOfView[2])
      end
    end)
  end
end

function _SetCameraEffects(nPlayerCam, sCamEffectState, nDuration, nAngle, nStartNear, nEndNear, nStartFar, nEndFar, nBlur)
  if sCamEffectState == "RestoreAll" then
    Graphics.Camera.RestoreFovParams(nPlayerCam, nDuration)
    Graphics.Camera.RestoreFocusParams(nPlayerCam, nDuration)
  elseif sCamEffectState == "RestoreFOV" then
    Graphics.Camera.RestoreFovParams(nPlayerCam, nDuration)
  elseif sCamEffectState == "RestoreDOF" then
    Graphics.Camera.RestoreFocusParams(nPlayerCam, nDuration)
  elseif sCamEffectState == "SetDOF" then
    Graphics.Camera.SetFocusParams(nPlayerCam, nStartNear, nEndNear, nStartFar, nEndFar, nBlur, nDuration)
  elseif sCamEffectState == "SetFOV" then
    Graphics.Camera.SetFovParams(nPlayerCam, nAngle, nDuration)
  end
end

function _SetDefaultCameraEffects()
  _SetCameraEffects(0, "SetDOF", 0, _tDefaultCameraEffects.DOF.nAngle, _tDefaultCameraEffects.DOF.nStartNear, _tDefaultCameraEffects.DOF.nEndNear, _tDefaultCameraEffects.DOF.nStartFar, _tDefaultCameraEffects.DOF.nEndFar, _tDefaultCameraEffects.DOF.nBlur)
  _SetCameraEffects(0, "SetFOV", 0, _tDefaultCameraEffects.FOV.nAngle)
end

function _Fade(bIn, fCallback, tData)
  Debug.Printf("---------------------------------Calling _Fade(" .. tostring(bIn) .. ")")
  Debug.Printf(Debug.GetCallstack())
  if _bFadedIn == bIn then
    Debug.Printf("---------------------------------Already in requested fade state (" .. tostring(bIn) .. ")")
    MrxUtil.CallWithOptionalArgs(fCallback, tData)
    return
  end
  Debug.Printf("---------------------------------Entering requested fade state (" .. tostring(bIn) .. ")")
  if bIn then
    MrxState.Exit(MrxState.STATE_WAITFORGAME, fCallback, tData)
  else
    MrxState.Enter(MrxState.STATE_WAITFORGAME, fCallback, tData)
  end
  _bFadedIn = bIn
end

NETEVENT_ENABLEMARKERS = 0
NETEVENT_DISABLEMARKERS = 1
NETEVENT_DISPLAYMENU = 2
NETEVENT_HIDEMENU = 3

function NetEventCallback(nEventType)
  if nEventType == NETEVENT_ENABLEMARKERS then
    Gui.EnablePlayerMarkers(true)
  elseif nEventType == NETEVENT_DISABLEMARKERS then
    Gui.EnablePlayerMarkers(false)
  elseif nEventType == NETEVENT_DISPLAYMENU then
    _DisplayClientMenu()
  elseif nEventType == NETEVENT_HIDEMENU then
    _CleanupClientMenu()
  end
end
