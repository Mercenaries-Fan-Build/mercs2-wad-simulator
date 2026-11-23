local L0_1, L1_1, L2_1
inherit("MrxMissionFlow", false)
import("MrxCinematic", false)
import("MrxFactionManager", false)
import("MrxGui", false)
import("MrxGuiBase", false)
import("MrxLayerManager", false)
import("MrxPlayState", false)
import("MrxStarterManager", false)
import("MrxState", false)
import("MrxSupportData", false)
import("MrxUtil", false)
import("MrxTransit", false)
import("MrxVoSequence", false)
import("Munitions", false)
import("MrxAchievements", false)
import("MrxVerifyManager", false)
import("WifHints", false)
import("MrxSoundBootstrap", false)
import("WifBios", false)
import("MrxSoundCategories", false)
import("MrxMusic", false)
import("WifMissionData", false)
import("MrxGuiDialogBox", false)
import("DLC01_MissionHub", true)

function L0_1()
  local L0_2, L1_2, L2_2, L3_2, L4_2, L5_2
  L1_2 = Player.GetPrimaryCharacter
  L1_2, L2_2, L3_2, L4_2, L5_2 = L1_2()
  L0_2 = MrxUtil.GetCharacterIdentity(L1_2, L2_2, L3_2, L4_2, L5_2)
  if L0_2 then
    L2_2 = string.sub
    L3_2 = L0_2
    L4_2 = 1
    L5_2 = 1
    L2_2, L3_2, L4_2, L5_2 = L2_2(L3_2, L4_2, L5_2)
    L0_2 = string.upper(L2_2, L3_2, L4_2, L5_2)
  end
  if L0_2 ~= "M" and L0_2 ~= "J" and L0_2 ~= "C" then
    L0_2 = "M"
  end
  L1_2 = {}
  L2_2 = {}
  
  function L3_2()
    local L0_3, L1_3
    L0_3 = true
    return L0_3
  end
  
  L2_2.fPrereq = L3_2
  
  function L3_2()
    local L0_3, L1_3, L2_3, L3_3
    _BeginBlockingSequence()
    
    function L0_3()
      local L0_4, L1_4, L2_4
      SetGrappleEnabled(true)
      SetVehicleDisguiseEnabled(true)
      EnableResourceCounters(true)
      MrxSupportData.SetHeliPilotRecruited(true)
      MrxSupportData.SetMechanicRecruited(true)
      MrxSupportData.SetJetPilotRecruited(true)
      UnlockMission("DlcCon001")
      UnlockMission("DlcCon002")
      UnlockMission("DlcCon003")
      UnlockMission("DlcCon004")
      L0_4 = MrxCheatBootstrap.IsSkipModeEnabled()
      if not L0_4 then
        MrxState.Exit(MrxState.STATE_WAITFORGAME, _EndBlockingSequence)
      else
        _EndBlockingSequence()
      end
    end
    
    L1_3 = MrxCheatBootstrap.IsSkipModeEnabled()
    if not L1_3 then
      MrxState.Enter(MrxState.STATE_WAITFORGAME, L0_3)
    else
      L0_3()
    end
  end
  
  L2_2.fConseq = L3_2
  L1_2.Start = L2_2
  return L1_2
end

GetOriginalFlowData = L0_1

function L0_1(A0_2, A1_2, A2_2)
  local L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2, L20_2, L21_2, L22_2, L23_2, L24_2, L25_2, L26_2, L27_2, L28_2, L29_2, L30_2, L31_2, L32_2, L33_2, L34_2
  L3_2 = A0_2
  A0_2 = GetCaseSensitiveMissionId(A0_2)
  if not A0_2 then
    L4_2 = false
    return L4_2
  end
  L3_2 = nil
  L4_2 = _oParent
  L4_2 = L4_2.GetChild(L4_2, A0_2)
  if L4_2 then
    L4_2 = false
    return L4_2
  end
  L4_2 = _tActiveMissions[A0_2]
  if L4_2 then
    L4_2 = false
    return L4_2
  end
  L4_2 = {}
  L5_2 = WifMissionData.tMissionData[A0_2]
  L6_2 = nil
  L7_2 = type(L5_2)
  if L7_2 == "table" then
    L6_2 = MrxUtil.CopyTable(L5_2)
  else
    L6_2 = {}
  end
  L7_2 = L6_2.sStarter
  L8_2 = nil
  if L7_2 then
    L8_2 = WifStarterData[L7_2]
  end
  L9_2 = WifMissionData.GetMissionTitle(A0_2)
  L10_2 = MrxTask
  L10_2 = L10_2.Create(L10_2)
  L11_2 = MrxTask
  L11_2 = L11_2.Create(L11_2)
  L4_2.sName = A0_2
  L4_2.sModuleName = "MrxTask"
  L4_2.oParent = _oParent
  
  function L12_2()
    local L0_3, L1_3, L2_3, L3_3, L4_3, L5_3, L6_3, L7_3, L8_3
    L0_3 = WifMissionData.IsMissionAContract(A0_2)
    if L0_3 then
      MrxPlayState.Set(MrxPlayState._knFree)
      WifMissionFlow.SetRetryLocations(nil)
    end
    L1_3 = L6_2.sStarter
    if L1_3 then
      L1_3 = MrxStarterManager.RequestStarter(L6_2.sStarter)
      if L1_3 then
        L1_3.RemoveBriefing(L1_3, A0_2)
      end
    end
    AwardKey(A0_2)
    L1_3 = GetKeyValue(A0_2)
    L2_3 = type(L6_2.tMilestones)
    if L2_3 == "table" then
      L2_3 = WifMissionData.IsMissionAJob(A0_2)
      if L2_3 then
        L2_3 = pairs
        L3_3 = L6_2.tMilestones
        L2_3, L3_3, L4_3 = L2_3(L3_3)
        for L5_3, L6_3 in L2_3, L3_3, L4_3 do
          L7_3 = HasKey(L6_3.sKey)
          if not L7_3 then
            AwardKey(L6_3.sKey)
          end
        end
      else
        L2_3 = pairs
        L3_3 = L6_2.tMilestones
        L2_3, L3_3, L4_3 = L2_3(L3_3)
        for L5_3, L6_3 in L2_3, L3_3, L4_3 do
          L7_3 = type(L6_3.nMilestone)
          if L7_3 == "number" then
            L7_3 = L6_3.nMilestone
            if L7_3 == L1_3 then
              AwardKey(L6_3.sKey)
            end
          end
        end
      end
    end
    L2_3 = _tActiveMissions
    L2_3[A0_2] = nil
    L2_3 = Net.IsServer()
    if L2_3 then
      L3_3 = WifMissionData.GetMissionIndexFromId
      L4_3 = A0_2
      L3_3, L4_3, L5_3, L6_3, L7_3, L8_3 = L3_3(L4_3)
      Net.SendEvent_RemovePDAMission(L3_3, L4_3, L5_3, L6_3, L7_3, L8_3)
    end
    L2_3 = Pda.Map
    L4_3 = {}
    L4_3.sName = A0_2
    L2_3.RemoveMission(L2_3, L4_3)
    UnlockMission(A0_2)
    Refresh()
    SetLastCompletedContractName(A0_2)
    DLC01_MissionHub.Enter(true)
  end
  
  L13_2 = {}
  L14_2 = {}
  L14_2[1] = L12_2
  L13_2[1] = L14_2
  L4_2.tOnComplete = L13_2
  
  function L13_2()
    local L0_3, L1_3, L2_3, L3_3, L4_3
    L0_3 = WifMissionData.IsMissionAContract(A0_2)
    if L0_3 then
      MrxPlayState.Set(MrxPlayState._knFree)
    end
    L1_3 = _tActiveMissions
    L1_3[A0_2] = nil
    L1_3 = L6_2.sStarter
    if L1_3 then
      UnlockMission(A0_2, nil, false)
    end
    DLC01_MissionHub.Enter(true)
  end
  
  L14_2 = {}
  L15_2 = {}
  L15_2[1] = L13_2
  L14_2[1] = L15_2
  L4_2.tOnCancel = L14_2
  L10_2.Configure(L10_2, L4_2)
  L14_2 = false
  L15_2 = nil
  if L8_2 then
    L14_2 = true
    L16_2 = type(A1_2) == "table"
    if not L16_2 then
      L17_2 = MrxTask
      L15_2 = L17_2.Create(L17_2)
    else
    end
  end
  L16_2 = MrxCheatBootstrap.IsSkipModeEnabled()
  L17_2 = MrxCheatBootstrap.GetMissionSkipData
  L17_2, L18_2 = L17_2()
  if L17_2 then
    L17_2 = GetCaseSensitiveMissionId(L17_2)
  end
  L6_2.sName = (A0_2 .. "Mission")
  L6_2.sModuleName = L6_2.sModuleName
  L6_2.oParent = L10_2
  L19_2 = L6_2.tLayers
  if not L19_2 then
    L19_2 = {}
    L19_2[1] = ("Vz_State_" .. A0_2)
  end
  L6_2.tLayers = L19_2
  L6_2.tRewards = MrxRewardData.GetRewards(A0_2)
  L6_2.tStartLocations = GetMissionStartLocations(A0_2)
  if L16_2 then
    L19_2 = WifMissionData.IsMissionAJob(A0_2)
    if L19_2 and L17_2 ~= A0_2 then
      L6_2.bSkipInitialNotifications = true
    end
  end
  if L15_2 then
    L6_2.oBriefing = L15_2
  end
  
  function L19_2()
    local L0_3, L1_3
    MrxState.Exit(MrxState.STATE_WAITFORSTREAMING)
    MrxState.Exit(MrxState.STATE_WAITFORGAME)
  end
  
  L6_2.fOnAssetsLoaded = L19_2
  
  function L20_2()
    local L0_3, L1_3
    L0_3 = L10_2
    L0_3.Complete(L0_3)
  end
  
  L21_2 = L6_2.tOnComplete
  if not L21_2 then
    L21_2 = {}
  end
  L6_2.tOnComplete = L21_2
  L23_2 = {}
  L23_2[1] = L20_2
  table.insert(L6_2.tOnComplete, L23_2)
  L21_2 = false
  L22_2 = L6_2.tRewards
  if L22_2 then
    L22_2 = L6_2.tRewards.nWager
    if not L22_2 then
      L22_2 = L6_2.tRewards.nWagerPercent
      if not L22_2 then
        goto lbl_188
      end
    end
    L21_2 = true
  end
  ::lbl_188::
  if L21_2 then
    function L22_2(A0_3, A1_3)
      local L2_3, L3_3, L4_3
      
      WifPmcInterior.SetEntranceLock(false)
      MrxHqManager.UnlockAllHq()
      WifPmcInterior.SetWagerStatus(A0_3, A1_3)
      WifPmcInterior.Enter(true)
    end
    
    L23_2 = L6_2.tOnCancel
    if not L23_2 then
      L23_2 = {}
    end
    L6_2.tOnCancel = L23_2
    L25_2 = {}
    L27_2 = {}
    L27_2[1] = A0_2
    L27_2[2] = false
    L25_2[1] = L22_2
    L25_2[2] = L27_2
    table.insert(L6_2.tOnCancel, L25_2)
    L23_2 = L6_2.tOnComplete
    if not L23_2 then
      L23_2 = {}
    end
    L6_2.tOnComplete = L23_2
    L25_2 = {}
    L27_2 = {}
    L27_2[1] = A0_2
    L27_2[2] = true
    L25_2[1] = L22_2
    L25_2[2] = L27_2
    table.insert(L6_2.tOnComplete, L25_2)
  end
  L11_2.Configure(L11_2, L6_2)
  
  function L22_2()
    local L0_3, L1_3, L2_3, L3_3, L4_3
    L0_3 = L6_2.sStarter
    if L0_3 then
      L0_3 = MrxStarterManager.RequestStarter(L6_2.sStarter)
      if L0_3 then
        L0_3.AddBriefing(L0_3, A0_2, L9_2)
      end
    end
  end
  
  function L23_2()
    local L0_3, L1_3, L2_3, L3_3, L4_3, L5_3
    L0_3 = _fPreContractSave
    if L0_3 then
      _fPreContractSave()
    end
    L0_3 = L6_2.sStarter
    if L0_3 then
      L0_3 = MrxStarterManager.RequestStarter(L6_2.sStarter)
      if L0_3 then
        L0_3.SetMissionAccepted(L0_3, A0_2, true)
        L0_3.RemoveBriefing(L0_3, A0_2)
      end
    end
    L0_3 = L21_2
    if L0_3 then
      WifPmcInterior.SetEntranceLock(true)
      MrxHqManager.LockAllHq()
    end
    L3_3 = {}
    L3_3[1] = L11_2
    L3_3[2] = A1_2
    MrxState.Enter(MrxState.STATE_WAITFORSTREAMING, L11_2.Activate, L3_3)
  end
  
  if L16_2 and L17_2 then
    L24_2 = _bSkipToMissionReached
    if not L24_2 then
      L24_2 = true
      L25_2 = WifMissionData.GetIsCompleteable(A0_2)
      if L25_2 then
        L24_2 = true
      else
        L24_2 = false
      end
      if (A0_2 == "ChiCon003" or A0_2 == "AllCon003") and L17_2 ~= "PmcCon004" then
        L24_2 = false
      end
      if A0_2 == L17_2 then
        _bSkipToMissionReached = true
        _AttemptSkipModeExit()
        L14_2 = L18_2
      elseif L24_2 then
        L25_2 = WifMissionData.IsMissionOnCriticalPath(A0_2)
        if not L25_2 then
          L25_2 = WifMissionData.IsMissionAContract(A0_2)
          if not L25_2 then
            goto lbl_296
          end
          L25_2 = WifMissionData.GetMissionFaction(A0_2)
          L26_2 = WifMissionData.GetMissionFaction(L17_2)
          if L25_2 ~= L26_2 then
            goto lbl_296
          end
        end
        L10_2.Complete(L10_2)
        L25_2 = true
        return L25_2
      end
    end
  end
  ::lbl_296::
  L10_2.Activate(L10_2)
  AddPdaMissionDetails(A0_2)
  A2_2 = MrxUtil.SetDefault(A2_2, true)
  
  function L24_2()
    local L0_3, L1_3
    L0_3 = A2_2
    if L0_3 then
      _EndBlockingSequence()
    end
  end
  
  if A2_2 then
    _BeginBlockingSequence()
  end
  if L14_2 then
    if L15_2 then
      L25_2 = {}
      L25_2.sModuleName = "MrxTask"
      L25_2.sName = (A0_2 .. "Briefing")
      L25_2.oParent = L10_2
      L25_2.sMissionName = A0_2
      L25_2.sFactionId = L6_2.sFactionId
      L25_2.fOnActivate = L24_2
      L25_2.fOnComplete = L23_2
      L26_2 = L8_2.sHqName
      if not L26_2 then
        L26_2 = L8_2.tLayers
        if L26_2 then
          L27_2 = L25_2.tLayers
          if not L27_2 then
            L27_2 = {}
          end
          L25_2.tLayers = L27_2
          L27_2 = ipairs
          L28_2 = L26_2
          L27_2, L28_2, L29_2 = L27_2(L28_2)
          for L30_2, L31_2 in L27_2, L28_2, L29_2 do
            table.insert(L25_2.tLayers, L31_2)
          end
          L26_2 = nil
        end
      end
      L15_2.Configure(L15_2, L25_2)
      L15_2.Activate(L15_2)
      L22_2()
    else
      L27_2 = {}
      L27_2.fOnActivate = L24_2
      L11_2.Configure(L11_2, L27_2)
      L22_2()
      L23_2()
    end
  else
    L27_2 = {}
    L27_2.fOnActivate = L24_2
    L11_2.Configure(L11_2, L27_2)
    L23_2()
  end
  L25_2 = _tActiveMissions
  L26_2 = {}
  L26_2.oMission = L11_2
  L25_2[A0_2] = L26_2
  L25_2 = true
  return L25_2
end

UnlockMission = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2
  L1_2 = type(A0_2)
  if L1_2 == "table" then
    _tMyFlowData = A0_2.tMyFlowData
    L1_2 = 0
    L2_2 = pairs
    L3_2 = A0_2.tCulledBindings
    L2_2, L3_2, L4_2 = L2_2(L3_2)
    for L5_2, L6_2 in L2_2, L3_2, L4_2 do
      L7_2 = _tFlowData[L6_2]
      if L7_2 then
        L7_2 = _tFlowData[L6_2]
        L7_2.bToBeCulled = true
        L1_2 = L1_2 + 1
      end
    end
    if 0 < L1_2 then
      L2_2 = {}
      L3_2 = pairs
      L4_2 = _tFlowData
      L3_2, L4_2, L5_2 = L3_2(L4_2)
      for L6_2, L7_2 in L3_2, L4_2, L5_2 do
        L8_2 = L7_2.bToBeCulled
        if not L8_2 then
          L2_2[L6_2] = L7_2
        else
          table.insert(_tCulledBindings, L6_2)
        end
      end
      _tFlowData = L2_2
    end
    L2_2 = pairs
    L3_2 = A0_2.tActiveMissions
    L2_2, L3_2, L4_2 = L2_2(L3_2)
    for L5_2, L6_2 in L2_2, L3_2, L4_2 do
      UnlockMission(L5_2, L6_2, false)
    end
  end
end

LoadSingleton = L0_1

function L0_1(A0_2)
  local L1_2, L2_2
  MrxMissionFlow.Reset(A0_2)
  SetFlowData(GetOriginalFlowData())
end

Reset = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2
  L2_2 = MrxCheatBootstrap.IsSkipModeEnabled()
  if not L2_2 then
    MrxUtil.CallWithOptionalArgs(A0_2, A1_2)
  else
    L4_2 = {}
    L4_2[1] = A0_2
    L4_2[2] = A1_2
    MrxLayerManager.ProcessMarkedLayers(MrxUtil.CallWithOptionalArgs, L4_2)
  end
end

_ChangeOutpostStaging = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2
  A0_2.SetSwfFile(A0_2, nil)
  MrxGui.RemoveWidget(A0_2)
  A0_2.delete(A0_2)
  MrxGui.FadeFromColor()
  Sys.RequestGameState("unloading")
  Net.QuitGame()
end

_ClientQuitToShell = L0_1
