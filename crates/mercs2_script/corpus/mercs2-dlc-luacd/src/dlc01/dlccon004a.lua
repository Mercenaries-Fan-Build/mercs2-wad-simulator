local L0_1, L1_1, L2_1, L3_1, L4_1, L5_1, L6_1, L7_1, L8_1, L9_1, L10_1, L11_1, L12_1, L13_1, L14_1, L15_1, L16_1, L17_1, L18_1, L19_1, L20_1, L21_1
inherit("MrxTaskContract", false)
inherit("DangerousBuilding", false)
import("DLCCopterDrop", false)
import("DLCEscalation", false)
import("DLCComboMeter", false)
import("WifVzBoundary", false)
import("DLCCon004_Cash", false)
import("MrxUtil", false)
import("MrxVoSequence", false)
import("DlcCopterDrop", false)
import("MrxMusic", false)
import("dlc_moonpatrol", false)
import("MrxTutorialManager", false)
import("DLC01_MissionHub", false)
L0_1 = {}
L1_1 = {}
L2_1 = {}
L2_1.sCommand = "AddTemplate"
L3_1 = {}
L3_1[1] = "Driving"
L3_1[2] = "Stopped"
L3_1[3] = "Offroad"
L2_1.tSituation = L3_1
L3_1 = {}
L5_1 = "M151 .50Cal (VZ) (DriverGunner)"
L3_1[1] = "Car"
L3_1[2] = L5_1
L3_1[3] = 1
L2_1.tTemplate = L3_1
L1_1[1] = L2_1
L2_1 = {}
L3_1 = {}
L3_1.sCommand = "UpdateDensity"
L4_1 = {}
L4_1[1] = "Driving"
L4_1[2] = "Stopped"
L4_1[3] = "Offroad"
L3_1.tSituation = L4_1
L4_1 = {}
L4_1[1] = "Car"
L4_1[2] = 2
L3_1.tDensity = L4_1
L2_1[1] = L3_1
L3_1 = {}
L4_1 = {}
L4_1.sCommand = "RemoveTemplate"
L5_1 = {}
L5_1[1] = "Driving"
L5_1[2] = "Stopped"
L5_1[3] = "Offroad"
L4_1.tSituation = L5_1
L4_1.tTemplate = "M151 Softtop (VZ) (Driver)"
L3_1[1] = L4_1
L5_1 = {}
L6_1 = {}
L6_1.sCommand = "AddTemplate"
L7_1 = {}
L7_1[1] = "Driving"
L7_1[2] = "Stopped"
L7_1[3] = "Offroad"
L7_1[4] = "Tank"
L6_1.tSituation = L7_1
L7_1 = {}
L9_1 = "Scorpion90 (Full)"
L7_1[1] = "Tank"
L7_1[2] = L9_1
L7_1[3] = 1
L6_1.tTemplate = L7_1
L7_1 = {}
L7_1[1] = "Tank"
L7_1[2] = 1
L6_1.tDensity = L7_1
L5_1[1] = L6_1
L6_1 = {}
L7_1 = {}
L7_1.sCommand = "UpdateDensity"
L8_1 = {}
L8_1[1] = "Driving"
L8_1[2] = "Stopped"
L8_1[3] = "Offroad"
L7_1.tSituation = L8_1
L8_1 = {}
L8_1[1] = "Car"
L8_1[2] = 4
L7_1.tDensity = L8_1
L6_1[1] = L7_1
L8_1 = {}
L9_1 = {}
L9_1.sCommand = "UpdateDensity"
L10_1 = {}
L10_1[1] = "Driving"
L10_1[2] = "Stopped"
L10_1[3] = "Offroad"
L9_1.tSituation = L10_1
L10_1 = {}
L10_1[1] = "Tank"
L10_1[2] = 2
L9_1.tDensity = L10_1
L8_1[1] = L9_1
L9_1 = {}
L10_1 = {}
L10_1.sCommand = "AddTemplate"
L11_1 = {}
L11_1[1] = "Heli"
L11_1[2] = "Stopped"
L10_1.tSituation = L11_1
L11_1 = {}
L13_1 = "Alouette3 Elite (Driver)"
L11_1[1] = "Heli"
L11_1[2] = L13_1
L11_1[3] = 1
L10_1.tTemplate = L11_1
L11_1 = {}
L11_1[1] = "Heli"
L11_1[2] = 1
L10_1.tDensity = L11_1
L9_1[1] = L10_1
L10_1 = {}
L11_1 = {}
L11_1.sCommand = "AddTemplate"
L12_1 = {}
L12_1[1] = "Driving"
L12_1[2] = "Offroad"
L11_1.tSituation = L12_1
L12_1 = {}
L14_1 = "Alouette3 Elite (Driver)"
L12_1[1] = "Heli"
L12_1[2] = L14_1
L12_1[3] = 1
L11_1.tTemplate = L12_1
L12_1 = {}
L12_1[1] = "Heli"
L12_1[2] = 1
L11_1.tDensity = L12_1
L10_1[1] = L11_1
L0_1[1] = L1_1
L0_1[2] = L2_1
L0_1[3] = L3_1
L0_1[4] = {}
L0_1[5] = L5_1
L0_1[6] = L6_1
L0_1[7] = {}
L0_1[8] = L8_1
L0_1[9] = L9_1
L0_1[10] = L10_1
L0_1[11] = {}
L0_1[12] = {}
L0_1[13] = {}
L0_1[14] = {}
L0_1[15] = {}
L0_1[16] = {}
L0_1[17] = {}
L0_1[18] = {}
L0_1[19] = {}
L0_1[20] = {}
L0_1[21] = {}
tLocalEscalationTable = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2
  L1_2 = {}
  L1_2[1] = "DLC01_DlcCon004"
  L2_2 = Net.IsClient()
  if not L2_2 then
    L5_2 = {}
    L5_2[1] = A0_2
    MrxLayerManager.Add(L1_2, LoadAssets2, L5_2)
  end
end

LoadAssets = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2
  L3_2 = Player.GetLocalCharacter
  L3_2, L4_2, L5_2, L6_2, L7_2, L8_2 = L3_2()
  EquipWeapons(A0_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2)
  L2_2 = Object.IsHibernated(Pg.GetGuidByName("DLCCON004_UberPanhard_01"))
  if L2_2 then
    L5_2 = {}
    L5_2[1] = L1_2
    L5_2[2] = "awake"
    L7_2 = {}
    L7_2[1] = A0_2
    A0_2._CreateEvent(A0_2, Event.ObjectHibernation, L5_2, A0_2.AssetsLoaded, L7_2)
  else
    A0_2.AssetsLoaded(A0_2)
  end
end

LoadAssets2 = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2
  MrxTaskContract.Activated(A0_2)
  A0_2._SetCancelMessage(A0_2, "")
  L1_2 = Hud.ResourceCounter
  L3_2 = {}
  L3_2.bSuppressCash = true
  L3_2.bSuppressFuel = true
  L1_2.SetSuppressed(L1_2, L3_2)
  MrxPmc.AddCashQty(-MrxPmc.GetCashQty(), nil, nil, false)
  Player.SetFuel(0)
  L1_2 = Hud.ResourceCounter
  L3_2 = {}
  L3_2.bSuppressCash = false
  L3_2.bSuppressFuel = false
  L1_2.SetSuppressed(L1_2, L3_2)
  L1_2 = Hud.ResourceCounter
  L3_2 = {}
  L3_2.nDuration = -1
  L1_2.Show(L1_2, L3_2)
  L1_2 = Hud.ResourceCounter
  L3_2 = {}
  L3_2.bSuppressCash = false
  L3_2.bSuppressFuel = true
  L1_2.SetSuppressed(L1_2, L3_2)
  L1_2 = DLCEscalation
  L1_2.tEscalationTable = tLocalEscalationTable
  nTimeRemaining = 0
  nLocalBldgCount = 0
  nCollectedTimePickups = 0
  bEnteredVehicle = false
  uDeadUberVehicle = nil
  nTotalCash = 0
  L1_2 = {}
  L2_2 = {}
  L4_2 = {}
  L5_2 = {}
  L7_2 = "M151 Softtop (VZ) (Driver)"
  L5_2[1] = "Car"
  L5_2[2] = L7_2
  L5_2[3] = 1
  L4_2[1] = L5_2
  L5_2 = {}
  L6_2 = {}
  L6_2[1] = "Car"
  L6_2[2] = 3
  L5_2[1] = L6_2
  L2_2[1] = "Driving"
  L2_2[2] = L4_2
  L2_2[3] = L5_2
  L3_2 = {}
  L5_2 = {}
  L6_2 = {}
  L8_2 = "M151 Softtop (VZ) (Driver)"
  L6_2[1] = "Car"
  L6_2[2] = L8_2
  L6_2[3] = 1
  L5_2[1] = L6_2
  L6_2 = {}
  L7_2 = {}
  L7_2[1] = "Car"
  L7_2[2] = 3
  L6_2[1] = L7_2
  L3_2[1] = "Stopped"
  L3_2[2] = L5_2
  L3_2[3] = L6_2
  L4_2 = {}
  L6_2 = {}
  L7_2 = {}
  L9_2 = "M151 Softtop (VZ) (Driver)"
  L7_2[1] = "Car"
  L7_2[2] = L9_2
  L7_2[3] = 1
  L6_2[1] = L7_2
  L7_2 = {}
  L8_2 = {}
  L8_2[1] = "Car"
  L8_2[2] = 3
  L7_2[1] = L8_2
  L4_2[1] = "Offroad"
  L4_2[2] = L6_2
  L4_2[3] = L7_2
  L5_2 = {}
  L7_2 = {}
  L8_2 = {}
  L10_2 = "M113 AA (VZ) (Full)"
  L8_2[1] = "Tank"
  L8_2[2] = L10_2
  L8_2[3] = 1
  L7_2[1] = L8_2
  L8_2 = {}
  L9_2 = {}
  L9_2[1] = "Tank"
  L9_2[2] = 1
  L8_2[1] = L9_2
  L5_2[1] = "Heli"
  L5_2[2] = L7_2
  L5_2[3] = L8_2
  L6_2 = {}
  L6_2[1] = "Tank"
  L6_2[2] = {}
  L6_2[3] = {}
  L1_2[1] = L2_2
  L1_2[2] = L3_2
  L1_2[3] = L4_2
  L1_2[4] = L5_2
  L1_2[5] = L6_2
  tLocalPursuitTable = L1_2
  L1_2 = DLCEscalation
  L1_2.tPursuitTable = tLocalPursuitTable
  WifVzBoundary.SetupBoundary("RGN_DLCCON004_Boundary", false)
  L1_2 = A0_2.GetNumCompletions(A0_2)
  DangerousBuilding.SetRarity("all", "always")
  A0_2.tComboData = DLCComboMeter.Combo_Init(A0_2, Player.GetLocalPlayer(), 5)
  L2_2 = MrxTimer
  L4_2 = {}
  L4_2.nStartTime = 120
  L4_2.nWarning = 30
  L4_2.iTray = 2
  L5_2 = {}
  L6_2 = {}
  L8_2 = {}
  L8_2[1] = A0_2
  L6_2[1] = _RoundComplete
  L6_2[2] = L8_2
  L5_2[1] = L6_2
  L4_2.tDoneCallbacks = L5_2
  L5_2 = {}
  L6_2 = {}
  L8_2 = {}
  L8_2[1] = A0_2
  L6_2[1] = _TimeWarningVo
  L6_2[2] = L8_2
  L5_2[1] = L6_2
  L4_2.tWarnCallbacks = L5_2
  oMyTimer = L2_2.Create(L2_2, L4_2)
  L2_2 = oMyTimer
  L2_2.Start(L2_2)
  L3_2 = A0_2
  L2_2 = A0_2._CreatePersistentEvent
  L4_2 = Event.ScriptEvent
  L5_2 = {}
  L6_2 = "TimeAdded"
  
  function L7_2(A0_3)
    local L1_3, L2_3
    L1_3 = oMyTimer
    L1_3 = 0 < L1_3.GetTime(L1_3)
    return L1_3
  end
  
  L5_2[1] = L6_2
  L5_2[2] = L7_2
  L7_2 = {}
  L7_2[1] = A0_2
  L7_2[2] = oMyTimer
  eTimeAdded = L2_2(L3_2, L4_2, L5_2, _AddTime, L7_2)
  DLCEscalation.StartPursuit(A0_2)
  InitCashCollectionRound(A0_2)
  L2_2 = Pg.GetGuidByName("DLCCON004_UberPanhard_01")
  A0_2.OnPlayerExitVehicle(A0_2, L2_2)
  A0_2.SetupVehicleDeathEvent(A0_2, L2_2)
  dlc_moonpatrol.SetImpulse(8.5)
  dlc_moonpatrol.SetPosition(0.05)
end

Activated = L0_1

function L0_1(A0_2)
  local L1_2, L2_2
  A0_2.oScoreBoard = nil
  MrxTaskContract.Complete(A0_2)
end

Complete = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2
  L1_2 = oMyTimer
  L1_2.Stop(L1_2)
  L1_2 = A0_2.oScoreBoard
  if L1_2 then
    L1_2 = A0_2.oScoreBoard
    L1_2.Close(L1_2)
    A0_2.oScoreBoard = nil
  end
  Event.Delete(eCashPickup)
  Event.Delete(eTimeAdded)
  DLCCon004_Cash.Cleanup()
  DLCEscalation.ClearPursuit(A0_2)
  DLCComboMeter.Combo_Cleanup(A0_2.tComboData)
  RestoreWeapons(A0_2, Player.GetLocalCharacter())
  WifVzBoundary.RemoveWorldBoundary()
  MrxTaskContract.Cleanup(A0_2)
end

Cleanup = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2
  L1_2 = Player.GetLocalCharacter()
  L2_2 = Object.GetPosition
  L3_2 = L1_2
  L2_2, L3_2, L4_2 = L2_2(L3_2)
  L5_2 = DLCCopterDrop.Create
  L6_2 = "GR"
  L11_2 = true
  L5_2, L6_2 = L5_2(L6_2, A0_2, L2_2, L3_2, L4_2, L11_2)
  L9_2 = tostring
  L10_2 = L6_2
  L9_2, L10_2, L11_2, L12_2 = L9_2(L10_2)
  table.insert(tPlacedUberVehicles, L9_2, L10_2, L11_2, L12_2)
  L7_2 = Hud.Radar
  L9_2 = {}
  L9_2.sName = ("UberVehicle" .. tostring(L6_2))
  L9_2.uGuid = L6_2
  L9_2.nR = 255
  L9_2.nG = 255
  L9_2.nB = 0
  L9_2.nWidth = 8
  L9_2.nHeight = 8
  L9_2.sTexture = "objective_action"
  L9_2.bSticky = true
  L9_2.bDontNetSync = true
  L7_2.AddObjective(L7_2, L9_2)
end

RespawnUberVehicle = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2
  L1_2 = 250000
  L4_2 = {}
  L4_2.sName = "Destroy"
  L4_2.sModuleName = "MrxTaskObjective"
  L4_2.sDspShortDesc = "[DLCCon004.UI.ObjFanfare1]"
  A0_2.CreateChild(A0_2, L4_2)
  L4_2 = {}
  L4_2.sName = "Collect"
  L4_2.sModuleName = "MrxTaskObjective"
  L4_2.sDspShortDesc = "[DLCCon004.UI.ObjFanfare2]"
  A0_2.CreateChild(A0_2, L4_2)
  L3_2 = {}
  L3_2[1] = "Fiona-In-Mission-Contract-Dlc04-16"
  MrxVoSequence.Start(L3_2)
  L2_2 = Pg.GetGuidByName("Arena_01")
  L3_2 = Object.GetPosition
  L4_2 = L2_2
  L3_2, L4_2, L5_2 = L3_2(L4_2)
  L6_2 = Event.CreatePersistent
  L7_2 = Event.ScriptEvent
  L8_2 = {}
  L9_2 = "CashPickup"
  
  function L10_2(A0_3)
    local L1_3
    L1_3 = true
    return L1_3
  end
  
  L8_2[1] = L9_2
  L8_2[2] = L10_2
  
  function L9_2(A0_3)
    local L1_3, L2_3, L3_3, L4_3, L5_3, L6_3, L7_3, L8_3, L9_3, L10_3, L11_3, L12_3
    L1_3 = A0_3[1]
    L2_3 = 0
    if L1_3 == 1 then
      L2_3 = 1000 * A0_2.tComboData.nCombo
    elseif L1_3 == 3 then
      L2_3 = 10000 * A0_2.tComboData.nCombo
    elseif L1_3 == 5 then
      L2_3 = 50000 * A0_2.tComboData.nCombo
    end
    MrxPmc.AddCashQty(L2_3, true, "[DLCCon004.UI.CashPickedUp]")
    L3_3 = Player.GetCash()
    L4_3 = L1_2
    if L3_3 >= L4_3 then
      L3_3 = "[green][DLCCon004.UI.CompletionFanfare] " .. MrxUtil.FormatMoney(L1_2)
      L4_3 = MessageBox
      L4_3.AddMessage(L4_3, L3_3, 0, 5, 0, nil, true, nil)
      L1_2 = (L1_2 * 2)
    end
  end
  
  L10_2 = {}
  L10_2[1] = tData
  eCashPickup = L6_2(L7_2, L8_2, L9_2, L10_2)
end

InitCashCollectionRound = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2
  L1_2 = MrxPmc.GetCashQty()
  L2_2 = (DLCComboMeter.nCurDestroyedBldgs * DLCComboMeter.nPeakCombo) * 500
  MrxPmc.AddCashQty(L2_2, "[DLCCon003.Display.scoreBonus]")
  L4_2 = MrxPmc.GetCashQty() - (L1_2 + L2_2)
  L5_2 = ""
  if L4_2 < 0 then
    L5_2 = "-"
  end
  L6_2 = "[DLCCon004.UI.score_collectedCash]: "
  L7_2 = MrxUtil.FormatMoney(L1_2)
  L8_2 = "[n][DLCCon004.UI.score_comboBonus]: "
  L9_2 = MrxUtil.FormatMoney(L2_2)
  L10_2 = "[n][Scoring.Misc]: "
  L13_2 = math.abs
  L14_2 = L4_2
  L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2 = L13_2(L14_2)
  L12_2 = MrxUtil.FormatMoney(L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2)
  L13_2 = "[n][green][Scoring.Total]: "
  L15_2 = MrxPmc.GetCashQty
  L15_2, L16_2, L17_2, L18_2, L19_2 = L15_2()
  L6_2 = L6_2 .. L7_2 .. L8_2 .. L9_2 .. L10_2 .. L5_2 .. L12_2 .. L13_2 .. MrxUtil.FormatMoney(L15_2, L16_2, L17_2, L18_2, L19_2) .. "[n]"
  L8_2 = Player.GetPrimaryPlayer()
  L10_2 = {}
  L11_2 = nil
  L12_2 = A0_2.Complete
  L13_2 = {}
  L13_2[1] = A0_2
  L14_2 = nil
  L15_2 = nil
  L16_2 = "center"
  L17_2 = "center"
  L18_2 = true
  L19_2 = nil
  A0_2.oScoreBoard = MrxGui.DisplayDialogBox(L8_2, L6_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2)
  L9_2 = MrxPmc.GetCashQty
  L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2 = L9_2()
  Net.LeaderboardPushScore("DlcCon004", L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2)
  L9_2 = MrxPmc.GetCashQty
  L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2 = L9_2()
  DLC01_MissionHub.SetPrevBest("DlcCon004", L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2)
end

_RoundComplete = L0_1

function L0_1()
  local L0_2, L1_2, L2_2, L3_2
  L0_2 = {}
  L0_2[1] = "Fiona-In-Mission-Contract-Dlc04-01"
  L0_2[2] = "Fiona-In-Mission-Contract-Dlc04-02"
  L0_2[3] = "Fiona-In-Mission-Contract-Dlc04-03"
  PlayRandomVo(L0_2, 1)
end

_TimeWarningVo = L0_1

function L0_1(A0_2, A1_2, A2_2)
  local L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2
  A1_2.AddTime(A1_2, A2_2[1])
  L3_2 = MessageBox
  L3_2.AddMessage(L3_2, "[green] [DLCCon004.UI.TimeExtended]", 0, 4, 0, nil, true, nil)
  nCollectedTimePickups = (nCollectedTimePickups + 1)
  DLCEscalation.ParseEscalationTable(nCollectedTimePickups)
  L3_2 = {}
  L3_2[1] = "Fiona-In-Mission-Contract-Dlc04-11"
  L3_2[2] = "Fiona-In-Mission-Contract-Dlc04-12"
  L3_2[3] = "Fiona-In-Mission-Contract-Dlc04-13"
  L3_2[4] = "Fiona-In-Mission-Contract-Dlc04-14"
  L3_2[5] = "Fiona-In-Mission-Contract-Dlc04-15"
  Sound.CueSound(0, "clockBell")
  L4_2 = A0_2._tEvents
  L7_2 = {}
  L7_2[1] = 0.8
  L9_2 = {}
  L9_2[1] = L3_2
  L9_2[2] = 0.25
  L4_2.eTimePickupVo = Event.Create(Event.TimerRelative, L7_2, PlayRandomVo, L9_2)
end

_AddTime = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2
  L2_2 = A0_2.tWeaponTables
  if not L2_2 then
    L2_2 = {}
  end
  A0_2.tWeaponTables = L2_2
  L2_2 = A0_2.tWeaponTables
  L2_2[A1_2] = Human.Inventory.GetAllWeapons(A1_2)
  L2_2 = {}
  L3_2 = Pg.GetGuidByName("Fuel-Air RPG")
  L4_2 = Pg.GetGuidByName("Grenade")
  L5_2 = Pg.GetGuidByName
  L6_2 = "C4"
  L5_2, L6_2 = L5_2(L6_2)
  L2_2[1] = L3_2
  L2_2[2] = L4_2
  L2_2[3] = L5_2
  L2_2[4] = L6_2
  Human.Inventory.SetAllWeapons(A1_2, L2_2)
  Object.SetInfiniteAmmo(A1_2, true)
end

EquipWeapons = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2
  L2_2 = A0_2.tWeaponTables[A1_2]
  if L2_2 then
    L5_2 = {}
    L5_2[1] = L2_2
    Human.Inventory.SetAllWeapons(A1_2, L5_2)
  end
  Object.SetInfiniteAmmo(A1_2, false)
end

RestoreWeapons = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2
  L2_2 = math.randf()
  if A1_2 > L2_2 then
    L2_2 = MrxUtil.GetRandomTableElement(A0_2)
    L4_2 = {}
    L4_2[1] = L2_2
    MrxVoSequence.Start(L4_2)
  end
end

PlayRandomVo = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2
  L3_2 = type(Object.GetHealth(A1_2))
  if L3_2 == "number" and 0 < L2_2 then
    L4_2 = A0_2
    L3_2 = A0_2.CreateChild
    L5_2 = {}
    L5_2.sName = "Enter car"
    L5_2.sModuleName = "MrxTaskObjectiveEnterVehicle"
    L5_2.sDspShortDesc = "[MecCon001.Objectives.enterVehicle]"
    L5_2.vTgtInclude = A1_2
    
    function L6_2()
      local L0_3, L1_3, L2_3
      L0_3 = A0_2
      L0_3.OnPlayerEnterVehicle(L0_3, A1_2)
    end
    
    L5_2.fOnComplete = L6_2
    L3_2 = L3_2(L4_2, L5_2)
  end
  MrxMusic.PlaySpecialMusic("Dlc_mu_ed_elevator-01")
end

OnPlayerExitVehicle = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2
  L2_2 = A0_2._tEvents
  L5_2 = {}
  L5_2[1] = Player.GetAnyCharacter()
  L5_2[2] = A1_2
  L5_2[3] = "d"
  L5_2[4] = "x"
  L7_2 = {}
  L7_2[1] = A0_2
  L7_2[2] = A1_2
  L2_2.eEnterUberVeh = Event.Create(Event.ObjectInSeat, L5_2, OnPlayerExitVehicle, L7_2)
  MrxMusic.PlaySpecialMusic("Dlc_mu_ed_rock-02")
  L2_2 = bEnteredVehicle
  if L2_2 == false then
    L4_2 = {}
    L4_2[1] = 5
    L5_2 = MrxTutorialManager.ShowMessage
    L6_2 = "[DLCCon001.UI.Jump]"
    L5_2, L6_2, L7_2, L8_2, L9_2 = L5_2(L6_2)
    Event.Create(Event.TimerRelative, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2)
    L4_2 = {}
    L4_2[1] = 15
    Event.Create(Event.TimerRelative, L4_2, MrxTutorialManager.HideMessage, {})
    bEnteredVehicle = true
  end
end

OnPlayerEnterVehicle = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2
  L2_2 = A0_2._tEvents
  L5_2 = {}
  L5_2[1] = A1_2
  L7_2 = {}
  L7_2[1] = A0_2
  L7_2[2] = false
  L2_2.eVehicleDestroyed = Event.Create(Event.ObjectDeath, L5_2, RespawnVehicle, L7_2)
  L2_2 = A0_2._tEvents
  L5_2 = {}
  L5_2[1] = A1_2
  L5_2[2] = "VehicleSinking"
  L7_2 = {}
  L7_2[1] = A0_2
  L7_2[2] = true
  L2_2.eVehicleSunk = Event.Create(Event.ObjectPhysicsEvent, L5_2, RespawnVehicle, L7_2)
end

SetupVehicleDeathEvent = L0_1

function L0_1(A0_2, A1_2, A2_2)
  local L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2, L20_2, L21_2, L22_2
  Event.Delete(A0_2._tEvents.eVehicleDestroyed)
  Event.Delete(A0_2._tEvents.eVehicleSunk)
  L3_2 = A2_2
  if A1_2 then
    L3_2 = Player.GetPrimaryCharacter()
  end
  L4_2 = Object.GetPosition
  L5_2 = L3_2
  L4_2, L5_2, L6_2 = L4_2(L5_2)
  Pg.Spawn("global_particle_flaresmoke_green", L4_2, L5_2, L6_2)
  L8_2 = DlcCopterDrop.Create
  L9_2 = "PMC"
  L8_2, L9_2 = L8_2(L9_2, "Panhard (DLCCON004)", L4_2, L5_2, L6_2, true)
  L10_2 = {}
  L10_2[1] = "Fiona-In-Mission-Contract-Dlc04-07"
  L10_2[2] = "Fiona-In-Mission-Contract-Dlc04-08"
  L10_2[3] = "Fiona-In-Mission-Contract-Dlc04-09"
  L10_2[4] = "Fiona-In-Mission-Contract-Dlc04-10"
  PlayRandomVo(L10_2, 1)
  A0_2.SetupVehicleDeathEvent(A0_2, L9_2)
  L14_2 = {}
  L14_2[1] = L9_2
  L14_2[2] = L4_2
  L14_2[3] = L5_2
  L14_2[4] = L6_2
  L14_2[5] = "<"
  L14_2[6] = 20
  L14_2[7] = false
  L14_2[8] = false
  L16_2 = {}
  L16_2[1] = A0_2
  L16_2[2] = L9_2
  A0_2._CreateEvent(A0_2, Event.ObjectProximity, L14_2, OnPlayerExitVehicle, L16_2)
end

RespawnVehicle = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2
  A0_2._SetCancelMessage(A0_2, "You Ran Out of Time")
  Combo_Cleanup(A0_2.tComboData)
  A0_2.Cancel(A0_2)
end

_TimerUp = L0_1
