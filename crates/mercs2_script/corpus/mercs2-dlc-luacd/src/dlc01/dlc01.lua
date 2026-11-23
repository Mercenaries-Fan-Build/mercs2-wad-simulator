local L0_1, L1_1, L2_1, L3_1, L4_1, L5_1, L6_1, L7_1, L8_1, L9_1, L10_1, L11_1
L0_1 = {}
L0_1[1] = "dlc01_CommonLocations"
L0_1[2] = "dlc01_terrain"
L0_1[3] = "dlc01_dlccon002_roads"
L0_1[4] = "dlc01_DlcCon004_roads"
L0_1[5] = "dlc01_LowResTerrain"
L0_1[6] = "dlc01_caicara_scrub"
L0_1[7] = "dlc01_caicara_foliage"
L0_1[8] = "dlc01_caicara_roads"
L0_1[9] = "dlc01_SpeedCity_scrub"
L0_1[10] = "dlc01_SpeedCity_foliage"
L0_1[11] = "dlc01_SpeedCity_roads"
_tStaticLayers = L0_1
L0_1 = {}
L0_1[1] = "DLC01_DLCCon002_Race"
L0_1[2] = "dlc01_SpeedCity"
L0_1[3] = "dlc01_caicara"
_tDefaultDynamicLayers = L0_1
import("Munitions", false)
import("MrxAchievements", false)
import("MrxBootstrap", false)
import("MrxCheatBootstrap", false)
import("MrxSoundBootstrap", false)
import("MrxFactionManager", false)
import("MrxLayerManager", false)
import("MrxStarterManager", false)
import("MrxPlayer", false)
import("MrxPlayState", false)
import("MrxPmc", false)
import("MrxTransit", false)
import("MrxTutorialManager", false)
import("MrxStatsManager", false)
import("MrxSupportData", false)
import("MrxRewardData", false)
import("MrxState", false)
import("MrxTask", false)
import("WifHints", false)
import("WifBios", false)
import("WifPmcInterior", false)
import("Hero", false)
import("MrxGui", false)
import("MrxUtil", false)
import("MrxVoSequence", false)
import("MrxShootingGallery", false)
import("MrxShop", false)
import("WifEquipmentData", false)
import("WifMissionFlow", false)
import("DLC01_MissionHub", true)
import("WifMissionData", false)
import("MrxGuiDialogBox", false)

function L0_1()
  local L0_2, L1_2, L2_2, L3_2
  _bAltSpawnLocation = L0_2
  _bBootstrapComplete = nil
  _bDynamicLayersLoaded = nil
  _bInitialStreamComplete = nil
  _bNewSession = nil
  _bPmcRequired = nil
  _bLoadIntoWorld = nil
  _bSkipToBriefing = nil
  _bStaticLayersLoaded = false
  _oMasterStub = nil
  _sSkipToMissionName = nil
  _tSaveState = nil
  _tTeleportLocations = nil
  _tPreContractSaveData = nil
  Sys.AddStringDb("patch01")
  Sys.AddStringDb("dlc01")
  MrxLayerManager.ResetState()
  MrxVoSequence.Reset()
  MrxBootstrap.SetHandleStateTransitions(false)
  L2_2 = {}
  L2_2[1] = "boot"
  MrxBootstrap.Start(_AttemptGameplaySetup, L2_2)
  L0_2 = Net.IsMultiplayer()
  if L0_2 then
    L0_2 = Net.IsServer()
    if not L0_2 then
      goto lbl_66
    end
  end
  MrxPlayer.Reset()
  ::lbl_66::
  MrxState.EnableFade(false)
  _bNewSession = true
  L0_2 = Pg.LoadGame("InitialSaveData")
  if not L0_2 then
    LoadSingleton(nil)
  end
  _bNewSession = nil
end

Init = L0_1

function L0_1()
  local L0_2, L1_2
  Player.SetAvailableCostumes(WifPmcInterior.GetAvailableCostumes())
  L0_2 = nil
  L0_2 = GenerateSaveData()
  return L0_2
end

SaveSingleton = L0_1
nInGameCash = 0

function L0_1()
  local L0_2, L1_2, L2_2, L3_2, L4_2, L5_2
  L0_2 = Net.IsClient()
  if not L0_2 then
    return
  end
  nInGameCash = Player.GetCash()
  L0_2 = nil
  Player.SetCash((nInGameCash + MrxPmc.GetClientReimburseAmount()))
  L3_2 = {}
  L3_2[1] = "Pause"
  L3_2[2] = "exit"
  Event.Create(Event.GameStateChange, L3_2, ClientRestorePreSaveCash, {})
end

ClientReimburseForSave = L0_1

function L0_1()
  local L0_2, L1_2
  L0_2 = Net.IsClient()
  if not L0_2 then
    return
  end
  L0_2 = nInGameCash
  if L0_2 then
    Player.SetCash(nInGameCash)
    nInGameCash = nil
  end
end

ClientRestorePreSaveCash = L0_1

function L0_1()
  local L0_2, L1_2
  L0_2 = _nOldCash
  if L0_2 then
    Player.SetCash(_nOldCash)
    _nOldCash = nil
  end
  L0_2 = _nOldFuel
  if L0_2 then
    Player.SetFuel(_nOldFuel)
    _nOldFuel = nil
  end
end

SaveComplete = L0_1

function L0_1()
  local L0_2, L1_2, L2_2, L3_2, L4_2
  L0_2 = {}
  L0_2.sLastMissionName = WifMissionFlow.GetLastCompletedContractName()
  L0_2.tPmcData = MrxPmc.SaveSingleton()
  L0_2.tSupportData = MrxSupportData.SaveSingleton()
  L0_2.tLayerData = MrxLayerManager.SaveSingleton()
  L0_2.tStarterData = MrxStarterManager.SaveSingleton()
  L0_2.tFlowData = WifMissionFlow.SaveSingleton()
  L0_2.tPlayerData = MrxPlayer.SaveSingleton()
  L0_2.tMunitionsData = Munitions.SaveSingleton()
  L0_2.nTimeElapsed = MrxPlayState.GetTotalTimeElapsed()
  L0_2.bGrappleEnabled = WifMissionFlow.IsGrappleEnabled()
  L0_2.bVehicleDisguiseEnabled = WifMissionFlow.IsVehicleDisguiseEnabled()
  L0_2.bResourceCountersEnabled = WifMissionFlow.AreResourceCountersEnabled()
  L0_2.tTutorialData = MrxTutorialManager.SaveSingleton()
  L0_2.nAvailableCostumes = WifPmcInterior.GetAvailableCostumes()
  L0_2.tStatsData = MrxStatsManager.SaveSingleton()
  L0_2.tShopData = MrxShop.SaveSingleton()
  L0_2.tWifEquipmentData = WifEquipmentData.SaveSingleton()
  L1_2 = Pda.Support
  L3_2 = {}
  L3_2.uPlayer = Player.GetPrimaryPlayer()
  L0_2.vEquippedSupport = L1_2.ReadEquippedSupport(L1_2, L3_2)
  L1_2 = WifMissionFlow.GetRetryLocations()
  if L1_2 then
    L0_2.tRetryLocations = L1_2
  end
  return L0_2
end

GenerateSaveData = L0_1

function L0_1()
  local L0_2, L1_2, L2_2
  _tPreContractSaveData = MrxUtil.CopyTable(GenerateSaveData())
end

SetPreContractSaveData = L0_1

function L0_1()
  local L0_2, L1_2
end

RequestAutosave = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2
  
  function L2_2(A0_3)
    local L1_3, L2_3, L3_3
    WifMissionFlow.Reset(true)
    MrxState.Reset()
    Graphics.SetShadowBaseDistance(10)
    Graphics.Camera.RestoreFocusParams(0, 1)
    Graphics.Camera.RestoreFovParams(0, 1)
    Graphics.Camera.RestoreNearFar(0)
    L1_3 = MrxGui
    L1_3.CleanupFadeFlash(L1_3)
    MrxPlayer.Reset()
    MrxPlayer.Deinit()
    Pg.ResetSingletonDone()
    MrxUtil.CallWithOptionalArgs(A0_2, A1_2)
    Pg.UnloadingStaticLayers(false)
    L1_3 = _fUnloadLayersCallback
    if L1_3 then
      _fUnloadLayersCallback(_tUnloadLayersCallbackData)
    end
  end
  
  MrxFactionManager.Reset()
  L3_2 = Pda
  L5_2 = {}
  L5_2.bSuppress = true
  L3_2.SetSuppressed(L3_2, L5_2)
  L3_2 = Player.GetAllPlayers()
  L4_2 = ipairs
  L5_2 = L3_2
  L4_2, L5_2, L6_2 = L4_2(L5_2)
  for L7_2, L8_2 in L4_2, L5_2, L6_2 do
    Human.ForceExitSeatNoSnap(Player.GetCharacter(L8_2))
  end
  L4_2 = Net.IsClient()
  if L4_2 then
    MrxPmc.NetClientReimburse()
  end
  MrxState.Reset()
  MrxPlayState.Reset()
  MrxVoSequence.Reset()
  WifMissionFlow.Reset(true)
  MrxShootingGallery.Reset()
  MrxSoundBootstrap.ExitGame()
  L4_2 = {}
  L5_2 = nil
  L6_2 = nil
  L7_2 = ipairs
  L8_2 = _tStaticLayers
  L7_2, L8_2, L9_2 = L7_2(L8_2)
  for L10_2, L11_2 in L7_2, L8_2, L9_2 do
    table.insert(L4_2, L11_2)
  end
  table.insert(L4_2, "dlc01_base")
  Pg.UnloadingStaticLayers(true)
  L10_2 = {}
  L10_2[1] = 1
  MrxLayerManager.RemoveAllLayers(L4_2, L2_2, L10_2)
  _tSaveState = nil
  _sSkipToMissionName = nil
  _tTeleportLocations = nil
end

ResetSingleton = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2
  L1_2 = Pg.GetUnloadingStaticLayers()
  if L1_2 then
    _fUnloadLayersCallback = LoadSingleton
    _tUnloadLayersCallbackData = A0_2
    return
  end
  _fUnloadLayersCallback = nil
  _tUnloadLayersCallbackData = nil
  L1_2 = Net.IsMultiplayer()
  if L1_2 then
    L1_2 = Net.IsServer()
    if not L1_2 then
      goto lbl_165
    end
  end
  MrxPlayState.Reset()
  MrxPlayState.Set(MrxPlayState._knFree)
  _tSaveState = A0_2
  SetupMissions()
  L1_2 = _tSaveState
  if not L1_2 then
    L1_2 = _sSkipToMissionName
    if not L1_2 then
      L1_2 = Sys.GetSkipMission()
      Sys.SetSkipMission("")
      L2_2 = type(L1_2)
      if L2_2 == "string" and L1_2 ~= "" then
        _sSkipToMissionName = L1_2
        _bSkipToBriefing = Sys.GetINIBriefing()
        MrxCheatBootstrap.EnableSkipMode(true, _sSkipToMissionName, _bSkipToBriefing)
      else
        L2_2 = Player.GetPlayerStart()
        if L2_2 ~= "PlayerLocation_Start" then
          _bAltSpawnLocation = true
        end
      end
    end
  end
  L1_2 = _bAltSpawnLocation
  if L1_2 then
    _bLoadIntoWorld = true
  else
    L1_2 = nil
    L2_2 = _tSaveState
    if L2_2 then
      L2_2 = Pg.LoadIsRetry()
      if L2_2 then
        L1_2 = WifMissionFlow.GetRetryLocations()
      else
        L2_2 = _tSaveState.tRetryLocations
        if L2_2 then
          WifMissionFlow.SetRetryLocations(_tSaveState.tRetryLocations)
          _bLoadIntoWorld = true
        else
          L2_2 = {}
          L2_2[1] = "Pmc_Entry1"
          L2_2[2] = "Pmc_Entry2"
          L1_2 = L2_2
          _bPmcRequired = true
        end
      end
    else
      L2_2 = nil
      L3_2 = _sSkipToMissionName
      if L3_2 then
        L2_2 = _sSkipToMissionName
      else
      end
      L3_2 = _sSkipToMissionName
      if L3_2 then
        L3_2 = _bSkipToBriefing
        if L3_2 then
          L1_2 = WifMissionFlow.GetBriefingStartLocations(L2_2)
        end
      end
      if not L1_2 then
        L3_2 = tDLCMissionData[L2_2]
        if L3_2 then
          L1_2 = tDLCMissionData[L2_2].tStartLocations
        end
      end
      if not L1_2 then
        L3_2 = {}
        L3_2[1] = "Starter_Pmc_Start1"
        L3_2[2] = "Starter_Pmc_Start2"
        L1_2 = L3_2
      end
      _bPmcRequired = true
    end
    L2_2 = type(L1_2)
    if L2_2 == "table" then
      L2_2 = _bNewSession
      if L2_2 then
        MrxPlayer.SetSpawnLocations(L1_2)
      else
        _tTeleportLocations = L1_2
        goto lbl_169
        ::lbl_165::
        Sys.SetSkipMission("")
      end
    end
  end
  ::lbl_169::
  L1_2 = Pg.LoadIsRetry()
  if L1_2 then
    L6_2 = {}
    L6_2[1] = "stream"
    MrxState.Enter(MrxState.STATE_WAITFORSTREAMING, _LoadLayers, nil, _AttemptGameplaySetup, L6_2)
  else
    _LoadLayers()
  end
end

LoadSingleton = L0_1

function L0_1()
  local L0_2, L1_2, L2_2, L3_2, L4_2, L5_2
  L0_2 = _bStaticLayersLoaded
  if not L0_2 then
    L3_2 = {}
    L3_2[1] = "static"
    MrxLayerManager.Add(_tStaticLayers, _AttemptGameplaySetup, L3_2, true, true)
  end
  L0_2 = Net.IsMultiplayer()
  if L0_2 then
    L0_2 = Net.IsServer()
    if not L0_2 then
      goto lbl_52
    end
  end
  L0_2 = Net.BeginLayerEventGroup
  if L0_2 then
    Net.BeginLayerEventGroup()
  end
  L0_2 = _tSaveState
  if L0_2 then
    L3_2 = {}
    L3_2[1] = "dynamic"
    MrxLayerManager.LoadSingleton(_tSaveState.tLayerData, _AttemptGameplaySetup, L3_2)
  else
    L3_2 = {}
    L3_2[1] = "dynamic"
    MrxLayerManager.Add(_tDefaultDynamicLayers, _AttemptGameplaySetup, L3_2)
  end
  ::lbl_52::
end

_LoadLayers = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2
  if A0_2 == "boot" then
    _bBootstrapComplete = true
  elseif A0_2 == "static" then
    _bStaticLayersLoaded = true
    MrxPlayer.Start()
  end
  L1_2 = _bBootstrapComplete or L1_2
  if L2_2 then
    L1_2 = _bStaticLayersLoaded
  end
  L2_2 = Net.IsMultiplayer()
  if L2_2 then
    L2_2 = Net.IsServer()
    if not L2_2 then
      goto lbl_68
    end
  end
  if A0_2 == "dynamic" then
    _bDynamicLayersLoaded = true
  end
  if L1_2 then
    L1_2 = _bDynamicLayersLoaded
  end
  L2_2 = Pg.LoadIsRetry()
  if L2_2 then
    if A0_2 == "stream" then
      _bInitialStreamComplete = true
    end
    if L1_2 then
      L1_2 = _bInitialStreamComplete
    end
  end
  if not L1_2 then
    return
  end
  _bDynamicLayersLoaded = nil
  MrxState.Enter(MrxState.STATE_WAITFORGAME, _CompleteGameplaySetup)
  L2_2 = _bInitialStreamComplete
  if L2_2 then
    MrxState.Exit(MrxState.STATE_WAITFORSTREAMING)
    _bInitialStreamComplete = nil
    goto lbl_73
    ::lbl_68::
    if not L1_2 then
      return
    end
    _GameplaySetup_LoadWorldState()
  end
  ::lbl_73::
end

_AttemptGameplaySetup = L0_1

function L0_1()
  local L0_2, L1_2
  _GameplaySetup_RestoreSubsystems()
  _GameplaySetup_LoadWorldState()
end

_CompleteGameplaySetup = L0_1

function L0_1()
  local L0_2, L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2
  L0_2 = Player.GetAllPlayers()
  L1_2 = ipairs
  L2_2 = L0_2
  L1_2, L2_2, L3_2 = L1_2(L2_2)
  for L4_2, L5_2 in L1_2, L2_2, L3_2 do
    L6_2 = Player.GetCharacter(L5_2)
    Hero.EndSurvivalMode(L5_2, L6_2, 0.1)
    Human.Inventory.ReloadAll(L6_2, false)
    Human.SetState(L6_2, "Upright", "Idle")
  end
  L1_2 = Pda.Support
  L3_2 = {}
  L3_2.sName = nil
  L3_2.sId = nil
  L1_2.SetEquippedItem(L1_2, L3_2)
  Sound.TransitionMusic("none")
  L1_2 = _tSaveState
  if L1_2 then
    MrxPlayer.LoadSingleton(_tSaveState.tPlayerData)
  end
end

_GameplaySetup_RestoreSubsystems = L0_1

function L0_1()
  local L0_2, L1_2, L2_2, L3_2, L4_2
  L0_2 = Net.IsClient()
  if L0_2 then
    Net.ApplyCachedFactionRelations()
    Net.SendEvent_RequestPosition()
    L0_2 = WifMissionFlow.AreResourceCountersEnabled()
    WifMissionFlow.EnableResourceCounters(false)
    MrxPmc.DisplayResources()
    WifMissionFlow.EnableResourceCounters(L0_2)
  end
  L0_2 = Net.IsMultiplayer()
  if L0_2 then
    L0_2 = Net.IsServer()
    if not L0_2 then
      goto lbl_187
    end
  end
  L0_2 = false
  L1_2 = _bAltSpawnLocation
  if not L1_2 then
    L1_2 = _sSkipToMissionName
    if not L1_2 then
      L0_2 = true
    end
  end
  L1_2 = _tSaveState
  if L1_2 then
    if L0_2 then
    end
    MrxSupportData.LoadSingleton(_tSaveState.tSupportData)
    WifMissionFlow.SetGrappleEnabled(_tSaveState.bGrappleEnabled)
    WifMissionFlow.SetVehicleDisguiseEnabled(_tSaveState.bVehicleDisguiseEnabled)
    MrxTutorialManager.LoadSingleton(_tSaveState.tTutorialData)
    MrxShop.LoadSingleton(_tSaveState.tShopData)
    WifEquipmentData.LoadSingleton(_tSaveState.tWifEquipmentData)
    WifMissionFlow.SetLastCompletedContractName(_tSaveState.sLastMissionName)
    WifPmcInterior.SetAvailableCostumes(_tSaveState.nAvailableCostumes)
    WifMissionFlow.EnableResourceCounters(false)
    MrxPmc.DisplayResources()
    WifMissionFlow.EnableResourceCounters(_tSaveState.bResourceCountersEnabled)
    L1_2 = _tSaveState.vEquippedSupport
    if L1_2 then
      L1_2 = Pda.Support
      L3_2 = {}
      L3_2.uPlayer = Player.GetPrimaryPlayer()
      L3_2.vSupport = _tSaveState.vEquippedSupport
      L1_2.RestoreEquippedSupport(L1_2, L3_2)
    end
  else
    if L0_2 then
    end
    MrxTransit.Reset()
    Munitions.SetMunitionsTaggable(false)
  end
  L1_2 = _oMasterStub
  if L1_2 then
    L1_2 = _oMasterStub
    L1_2.Cleanup(L1_2)
  end
  L1_2 = MrxTask
  _oMasterStub = L1_2.Create(L1_2)
  L1_2 = _oMasterStub
  L3_2 = {}
  L3_2.sName = "Missions"
  L1_2.Configure(L1_2, L3_2)
  L1_2 = _oMasterStub
  L1_2.Activate(L1_2)
  WifMissionFlow.Reset()
  WifMissionFlow.SetMissionParent(_oMasterStub)
  WifMissionFlow.SetPreContractSaveFunction(SetPreContractSaveData)
  MrxCheatBootstrap.SetTaskTreeRoot(_oMasterStub)
  MrxCheatBootstrap.SetMissionSkipDialogCallback(SkipToMission)
  L1_2 = _tTeleportLocations
  if L1_2 then
    MrxUtil.TeleportHeroesToLocations(_tTeleportLocations, _SecondaryStreamComplete)
    _tTeleportLocations = nil
    L1_2 = Pg.LoadIsRetry()
    if L1_2 then
      L3_2 = {}
      L3_2[1] = false
      Event.Post("parkingLotStart", L3_2)
    end
    return
  end
  ::lbl_187::
  MrxState.Enter(MrxState.STATE_WAITFORSTREAMING, nil, nil, _SecondaryStreamComplete)
end

_GameplaySetup_LoadWorldState = L0_1

function L0_1()
  local L0_2, L1_2
  MrxState.Exit(MrxState.STATE_WAITFORSTREAMING)
  _StartPlayerVisibleGameplay()
end

_SecondaryStreamComplete = L0_1

function L0_1()
  local L0_2, L1_2, L2_2, L3_2, L4_2, L5_2
  Sound.TransitionMusic("none")
  L0_2 = _tSaveState
  if L0_2 then
    WifMissionFlow.LoadSingleton(_tSaveState.tFlowData)
    MrxStarterManager.LoadSingleton(_tSaveState.tStarterData)
    MrxPlayState.SetTimeElapsedInPriorSessions(_tSaveState.nTimeElapsed)
    MrxStatsManager.LoadSingleton(_tSaveState.tStatsData)
    _tSaveState = nil
  else
    MrxPlayState.SetTimeElapsedInPriorSessions(0)
  end
  L0_2 = MrxPlayState.GetSessionTimer()
  if not L0_2 then
    MrxPlayState.StartSessionTimer()
  end
  L0_2 = Net.IsMultiplayer()
  if L0_2 then
    L0_2 = Net.IsServer()
    if not L0_2 then
      goto lbl_59
    end
  end
  MrxStatsManager.DeleteWeaponTimer()
  MrxStatsManager.AddWeaponTimer()
  ::lbl_59::
  L0_2 = _bAltSpawnLocation
  if not L0_2 then
    Debug.Printf("********* MY LOCAL VERSION HERE")
    L2_2 = {}
    L2_2[1] = MrxState.STATE_WAITFORGAME
    WifMissionFlow.Refresh(MrxState.Exit, L2_2)
  else
    MrxState.Exit(MrxState.STATE_WAITFORGAME)
  end
  L0_2 = Net.EndLayerEventGroup
  if L0_2 then
    Net.EndLayerEventGroup()
  end
  L0_2 = _bPmcRequired
  if L0_2 then
    function L0_2()
      local L0_3, L1_3, L2_3, L3_3
      
      L2_3 = {}
      L2_3[1] = MrxState.STATE_WAITFORGAME
      DLC01_MissionHub.SetLoadCallback(MrxState.Exit, L2_3)
      DLC01_MissionHub.Enter(true)
    end
    
    L1_2 = DLC01_MissionHub.IsInside()
    if L1_2 then
      DLC01_MissionHub.SetUnloadCallback(L0_2)
      DLC01_MissionHub.Exit(1, true)
    else
      L0_2()
    end
  end
  L0_2 = pairs
  L1_2 = MrxTutorialManager._tTutorials
  L0_2, L1_2, L2_2 = L0_2(L1_2)
  for L3_2, L4_2 in L0_2, L1_2, L2_2 do
    L4_2.bComplete = true
  end
  MrxTutorialManager.Setup()
  _bPmcRequired = nil
  _bLoadIntoWorld = nil
  _sSkipToMissionName = nil
  _bSkipToBriefing = nil
end

_StartPlayerVisibleGameplay = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2
  Sys.SetSkipMission(A0_2)
  Sys.SetINIBriefing(A1_2)
  Sys.RequestGameState("unloading")
end

SkipToMission = L0_1
L0_1 = {}
L1_1 = {}
L1_1.sModuleName = "DlcCon001"
L1_1.sFactionId = "Pmc"
L1_1.sStarter = "HubFiona"
L2_1 = {}
L2_1[1] = "loc_start_dlccon001_1"
L2_1[2] = "loc_start_dlccon001_2"
L1_1.tStartLocations = L2_1
L2_1 = {}
L2_1[1] = "dlc01_DlcCon001"
L1_1.tLayers = L2_1
L1_1.nLevels = 1
L1_1.bPlayerVisibleMission = true
L1_1.bContract = true
L0_1.DlcCon001 = L1_1
L1_1 = {}
L1_1.sModuleName = "DlcCon002"
L1_1.sFactionId = "Pmc"
L1_1.sStarter = "HubEva"
L2_1 = {}
L2_1[1] = "loc_start_dlccon002_1"
L2_1[2] = "loc_start_dlccon002_2"
L1_1.tStartLocations = L2_1
L2_1 = {}
L2_1[1] = "dlc01_dlccon002_race"
L1_1.tLayers = L2_1
L1_1.nLevels = 1
L1_1.bPlayerVisibleMission = true
L1_1.bContract = true
L0_1.DlcCon002 = L1_1
L1_1 = {}
L1_1.sModuleName = "DlcCon003"
L1_1.sFactionId = "Pmc"
L1_1.sStarter = "HubEwan"
L2_1 = {}
L2_1[1] = "loc_start_dlccon003_1"
L2_1[2] = "loc_start_dlccon003_2"
L1_1.tStartLocations = L2_1
L1_1.nLevels = 1
L1_1.bPlayerVisibleMission = true
L1_1.bContract = true
L0_1.DlcCon003 = L1_1
L1_1 = {}
L1_1.sModuleName = "DlcCon004a"
L1_1.sFactionId = "Pmc"
L1_1.sStarter = "HubMisha"
L2_1 = {}
L2_1[1] = "loc_start_dlccon004_1"
L2_1[2] = "loc_start_dlccon004_2"
L1_1.tStartLocations = L2_1
L2_1 = {}
L2_1[1] = "dlc01_dlccon004"
L1_1.tLayers = L2_1
L1_1.nLevels = 1
L1_1.bPlayerVisibleMission = true
L1_1.bContract = true
L0_1.DlcCon004 = L1_1
tDLCMissionData = L0_1

function L0_1()
  local L0_2, L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2
  WifMissionData.SetMissionData(tDLCMissionData)
  tMissionNames = {}
  L0_2 = pairs
  L1_2 = tDLCMissionData
  L0_2, L1_2, L2_2 = L0_2(L1_2)
  for L3_2, L4_2 in L0_2, L1_2, L2_2 do
    table.insert(tMissionNames, L3_2)
  end
  table.sort(tMissionNames)
end

SetupMissions = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2
  L1_2 = MrxCheatBootstrap.GetMissionSkipData()
  MrxCheatBootstrap.EnableSkipMode(false)
  if L1_2 then
    L2_2 = tDLCMissionData[L1_2]
    if L2_2 then
      MissionSelected(L1_2)
  end
  else
    MrxGuiDialogBox.DisplayDialogBox(Player.GetPrimaryPlayer(), "Choose a mission", tMissionNames, 1, MissionSelected, {})
  end
end

ShowMissionList = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2
  L1_2 = _oContainer
  if L1_2 then
    L1_2 = _oContainer
    L1_2.Cleanup(L1_2)
  end
  L1_2 = nil
  L2_2 = type(A0_2)
  if L2_2 == "number" then
    L1_2 = tMissionNames[A0_2]
  else
    L1_2 = A0_2
  end
  L2_2 = MrxTask
  _oContainer = L2_2.Create(L2_2)
  L2_2 = _oContainer
  L4_2 = {}
  L4_2.sName = L1_2
  L2_2.Configure(L2_2, L4_2)
  L2_2 = _oContainer
  L2_2.Activate(L2_2)
  L2_2 = tDLCMissionData[L1_2]
  L2_2.bRepeatable = true
  L2_2.sName = L1_2
  L2_2.oParent = _oContainer
  L3_2 = {}
  L4_2 = {}
  L6_2 = {}
  L6_2[1] = true
  L4_2[1] = ShowMissionList
  L4_2[2] = L6_2
  L3_2[1] = L4_2
  L2_2.tOnComplete = L3_2
  L3_2 = MrxTask
  L3_2 = L3_2.Create(L3_2)
  L3_2.Configure(L3_2, L2_2)
  L4_2 = type(L2_2.tStartLocations)
  if L4_2 == "table" then
    L7_2 = {}
    L7_2[1] = L3_2
    MrxUtil.TeleportHeroesToLocations(L2_2.tStartLocations, L3_2.Activate, L7_2)
  else
    L3_2.Activate(L3_2)
  end
end

MissionSelected = L0_1
