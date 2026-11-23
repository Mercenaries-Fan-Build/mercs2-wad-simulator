local L0_1, L1_1, L2_1
inherit("MrxTaskContract", false)
import("MrxTaskRace", false)
import("MrxMusic", false)
import("MrxPmc", false)
import("MrxUtil", false)
import("MrxTutorialManager", false)
import("DLC01_MissionHub", false)
kTimeCash = 10000
kVZSoldierCash = 2000

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2
  L2_2 = {}
  L2_2[1] = "DLC01_DLCCon002_Race"
  L6_2 = {}
  L6_2[1] = A0_2
  MrxLayerManager.Add(L2_2, A0_2.LoadAssets2, L6_2)
end

LoadAssets = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2
  L1_2 = Pg.GetGuidByName("Monster1")
  uTruck = L1_2
  L2_2 = Object.IsHibernated(L1_2)
  if L2_2 then
    L3_2 = A0_2
    L2_2 = A0_2._CreateEvent
    L4_2 = Event.ObjectHibernation
    L5_2 = {}
    L5_2[1] = L1_2
    L5_2[2] = "awake"
    
    function L6_2()
      local L0_3, L1_3, L2_3, L3_3, L4_3
      Vehicle.Enter(L1_2, Player.GetPrimaryCharacter(), "d", true)
      A0_2.AssetsLoaded(A0_2)
    end
    
    L2_2(L3_2, L4_2, L5_2, L6_2)
  else
    Vehicle.Enter(L1_2, Player.GetPrimaryCharacter(), "d", true)
    A0_2.AssetsLoaded(A0_2)
  end
end

LoadAssets2 = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2
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
  L2_2 = {}
  L2_2[1] = "Fiona-In-Mission-Contract-Oil020-12"
  MrxVoSequence.Start(L2_2)
  A0_2.StartRace(A0_2)
  A0_2.CheckPointVO(A0_2)
  MrxMusic.PlaySpecialMusic("Dlc_mu_ali")
  nVZSoldierCounter = 0
  L1_2 = Player.GetLocalCharacter()
  L2_2 = Pg.GetGuidByName("Monster1")
  L3_2 = Event.Create
  L4_2 = Event.ObjectInSeat
  L5_2 = {}
  L5_2[1] = L1_2
  L5_2[2] = L2_2
  L5_2[3] = "D"
  L5_2[4] = "E"
  
  function L6_2()
    local L0_3, L1_3, L2_3, L3_3, L4_3
    L0_3 = Hud.ObjectiveTray
    L2_3 = {}
    L2_3.nSlot = 2
    L2_3.sText = ("[DLCCon002.UI.KillCounter] " .. nVZSoldierCounter)
    L0_3.SetSlotToText(L0_3, L2_3)
  end
  
  eEnterVehicle = L3_2(L4_2, L5_2, L6_2, {})
  L5_2 = {}
  L5_2[1] = Player.GetLocalPlayer()
  L5_2[2] = "lbutton"
  L5_2[3] = "press"
  L5_2[4] = true
  evFlipVehicle = Event.CreatePersistent(Event.Button, L5_2, FlipVehicle, {})
  L5_2 = {}
  L5_2[1] = 1
  evVehicleFlipped = Event.CreatePersistent(Event.TimerRelative, L5_2, CheckFlipped, {})
  uVZFilter = ObjectFilter.Create()
  ObjectFilter.SetFilter(uVZFilter, "Human&&VZ")
  L3_2 = Event.CreatePersistent
  L4_2 = Event.ObjectDeath
  L5_2 = {}
  L5_2[1] = uVZFilter
  
  function L6_2()
    local L0_3, L1_3, L2_3, L3_3, L4_3
    nVZSoldierCounter = (nVZSoldierCounter + 1)
    L0_3 = Hud.ObjectiveTray
    L2_3 = {}
    L2_3.nSlot = 2
    L2_3.sText = ("[DLCCon002.UI.KillCounter] " .. nVZSoldierCounter)
    L0_3.SetSlotToText(L0_3, L2_3)
    MrxPmc.AddCashQty(kVZSoldierCash)
  end
  
  eVZSoldierDestroyed = L3_2(L4_2, L5_2, L6_2, {})
end

Activated = L0_1

function L0_1()
  local L0_2, L1_2, L2_2, L3_2, L4_2, L5_2
  L1_2 = Vehicle.IsFlipped(Pg.GetGuidByName("Monster1"))
  if L1_2 then
    L1_2 = Object.GetVelocity(L0_2)
    if L1_2 < 5 then
      MrxTutorialManager.ShowMessage("[DLCCon001.UI.FlipCar]")
      L3_2 = {}
      L3_2[1] = 10
      Event.Create(Event.TimerRelative, L3_2, MrxTutorialManager.HideMessage, {})
    end
  end
end

CheckFlipped = L0_1

function L0_1()
  local L0_2, L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2
  L1_2 = Vehicle.IsFlipped(Pg.GetGuidByName("Monster1"))
  if L1_2 then
    L1_2 = Object.GetVelocity(L0_2)
    if L1_2 < 5 then
      L1_2 = Object.GetMass(L0_2)
      Object.ApplyAngularImpulse(L0_2, (-L1_2 * 2.5), 0, (L1_2 * 2.5), true)
  end
  else
    return
  end
  L3_2 = {}
  L3_2[1] = 0.25
  Event.Create(Event.TimerRelative, L3_2, FlipVehicle, {})
end

FlipVehicle = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2
  L1_2 = {}
  L3_2 = {}
  L5_2 = {}
  L5_2[1] = A0_2
  L3_2[1] = A0_2.Cancel
  L3_2[2] = L5_2
  L1_2[1] = "Fiona-In-Mission-Contract-Dlc02-19"
  L1_2[2] = L3_2
  A0_2._SetCancelMessage(A0_2, "[ChiCon009.Terms.Cancel05]")
  MrxVoSequence.Start(L1_2)
end

MonsterTruckDestroyed = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2, L20_2, L21_2, L22_2, L23_2, L24_2, L25_2, L26_2, L27_2, L28_2, L29_2, L30_2, L31_2, L32_2, L33_2, L34_2, L35_2, L36_2, L37_2, L38_2, L39_2, L40_2, L41_2, L42_2, L43_2, L44_2
  L1_2 = 45
  L3_2 = A0_2
  L2_2 = A0_2.CreateChild
  L4_2 = {}
  L4_2.sName = "MecRace"
  L4_2.sModuleName = "MrxTaskRace"
  L4_2.bUseTripWires = true
  L4_2.bUseTenths = true
  L4_2.fWidth = 15
  L5_2 = {}
  L5_2.nStartTime = 30
  L4_2.tTimerParams = L5_2
  L4_2.nAddTime = 7
  L4_2.sGateType = "gate"
  L4_2.vTgtInclude = Pg.GetGuidByName("Monster1")
  L5_2 = {}
  L5_2[1] = "Fiona-In-Mission-Contract-Dlc02-15"
  L5_2[2] = 0.2
  L5_2[3] = "Fiona-In-Mission-Contract-Dlc02-11"
  L5_2[4] = 0.12
  L5_2[5] = "Fiona-In-Mission-Contract-Dlc02-09"
  L5_2[6] = 0.5
  L4_2.vVoSeqOnAdd = L5_2
  L5_2 = {}
  L5_2[1] = "check0"
  L5_2[2] = "check0a"
  L5_2[3] = "check1"
  L5_2[4] = "check2"
  L5_2[5] = "check3"
  L5_2[6] = "check4"
  L5_2[7] = "check5"
  L5_2[8] = "check6"
  L5_2[9] = "check7"
  L5_2[10] = "check8"
  L5_2[11] = "check9"
  L5_2[12] = "check10"
  L5_2[13] = "check11"
  L5_2[14] = "check12"
  L5_2[15] = "check13"
  L5_2[16] = "check14"
  L5_2[17] = "check15"
  L5_2[18] = "check16"
  L5_2[19] = "check17"
  L5_2[20] = "check18"
  L5_2[21] = "check19"
  L5_2[22] = "check20"
  L5_2[23] = "check21"
  L5_2[24] = "check22"
  L5_2[25] = "check22b"
  L5_2[26] = "check22c"
  L5_2[27] = "check23"
  L5_2[28] = "check24"
  L5_2[29] = "check25"
  L5_2[30] = "check26"
  L5_2[31] = "check27"
  L5_2[32] = "check28"
  L5_2[33] = "check29"
  L5_2[34] = "check30"
  L5_2[35] = "check31"
  L5_2[36] = "check32"
  L5_2[37] = "check33"
  L5_2[38] = "check34"
  L5_2[39] = "check35"
  L4_2.tCourseLocs = L5_2
  
  function L5_2()
    local L0_3, L1_3
    L0_3 = Object.IsAlive(uTruck)
    if L0_3 then
      L0_3 = A0_2
      L0_3.CourseUnfinished(L0_3)
    else
      L0_3 = A0_2
      L0_3.MonsterTruckDestroyed(L0_3)
    end
  end
  
  L4_2.fOnCancel = L5_2
  
  function L5_2()
    local L0_3, L1_3, L2_3, L3_3, L4_3, L5_3, L6_3, L7_3, L8_3, L9_3, L10_3, L11_3, L12_3, L13_3, L14_3, L15_3, L16_3, L17_3, L18_3, L19_3, L20_3
    L1_3 = A0_2.oRace._oTimer
    L2_3 = L1_3
    L1_3 = L1_3.GetTime
    L1_3, L2_3, L3_3, L4_3, L5_3, L6_3, L7_3, L8_3, L9_3, L10_3, L11_3, L12_3, L13_3, L14_3, L15_3, L16_3, L17_3, L18_3, L19_3, L20_3 = L1_3(L2_3)
    L1_3 = math.floor(L1_3, L2_3, L3_3, L4_3, L5_3, L6_3, L7_3, L8_3, L9_3, L10_3, L11_3, L12_3, L13_3, L14_3, L15_3, L16_3, L17_3, L18_3, L19_3, L20_3) * kTimeCash
    MrxPmc.AddCashQty(L1_3, "[Fanfare.Completion.TimeBonus]")
    L4_3 = MrxPmc.GetCashQty() - (L1_3 + (nVZSoldierCounter * kVZSoldierCash))
    L5_3 = ""
    if L4_3 < 0 then
      L5_3 = "-"
    end
    L6_3 = "[DLCCon002.UI.TimeRemaining] "
    L7_3 = MrxUtil.FormatMoney(L1_3)
    L8_3 = "[n][DLCCon002.UI.KillCounter] "
    L9_3 = MrxUtil.FormatMoney(L2_3)
    L10_3 = "[n][Scoring.Misc] : "
    L13_3 = math.abs
    L14_3 = L4_3
    L13_3, L14_3, L15_3, L16_3, L17_3, L18_3, L19_3, L20_3 = L13_3(L14_3)
    L12_3 = MrxUtil.FormatMoney(L13_3, L14_3, L15_3, L16_3, L17_3, L18_3, L19_3, L20_3)
    L13_3 = "[n][green][Scoring.Total] : "
    L15_3 = MrxPmc.GetCashQty
    L15_3, L16_3, L17_3, L18_3, L19_3, L20_3 = L15_3()
    L6_3 = L6_3 .. L7_3 .. L8_3 .. L9_3 .. L10_3 .. L5_3 .. L12_3 .. L13_3 .. MrxUtil.FormatMoney(L15_3, L16_3, L17_3, L18_3, L19_3, L20_3) .. "[n]"
    L7_3 = A0_2
    L9_3 = Player.GetPrimaryPlayer()
    L10_3 = L6_3
    L11_3 = {}
    L12_3 = nil
    L13_3 = A0_2.Complete
    L14_3 = {}
    L14_3[1] = A0_2
    L15_3 = nil
    L16_3 = nil
    L17_3 = "center"
    L18_3 = "center"
    L19_3 = true
    L20_3 = nil
    L7_3.oScoreBoard = MrxGui.DisplayDialogBox(L9_3, L10_3, L11_3, L12_3, L13_3, L14_3, L15_3, L16_3, L17_3, L18_3, L19_3, L20_3)
    L9_3 = MrxPmc.GetCashQty
    L9_3, L10_3, L11_3, L12_3, L13_3, L14_3, L15_3, L16_3, L17_3, L18_3, L19_3, L20_3 = L9_3()
    Net.LeaderboardPushScore("DlcCon002", L9_3, L10_3, L11_3, L12_3, L13_3, L14_3, L15_3, L16_3, L17_3, L18_3, L19_3, L20_3)
    L9_3 = MrxPmc.GetCashQty
    L9_3, L10_3, L11_3, L12_3, L13_3, L14_3, L15_3, L16_3, L17_3, L18_3, L19_3, L20_3 = L9_3()
    DLC01_MissionHub.SetPrevBest("DlcCon002", L9_3, L10_3, L11_3, L12_3, L13_3, L14_3, L15_3, L16_3, L17_3, L18_3, L19_3, L20_3)
  end
  
  L4_2.fOnComplete = L5_2
  A0_2.oRace = L2_2(L3_2, L4_2)
end

StartRace = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2
  A0_2._SetCancelMessage(A0_2, "[PmcCon015.Terms.Cancel04]")
  L2_2 = {}
  L4_2 = {}
  L6_2 = {}
  L6_2[1] = A0_2
  L4_2[1] = A0_2.Cancel
  L4_2[2] = L6_2
  L2_2[1] = "Fiona-In-Mission-Contract-Dlc02-18"
  L2_2[2] = L4_2
  MrxVoSequence.Start(L2_2)
end

CourseUnfinished = L0_1

function L0_1(A0_2)
  local L1_2, L2_2
  A0_2.oScoreBoard = nil
  MrxTaskContract.Complete(A0_2)
end

Complete = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2
  Event.Delete(evFlipVehicle)
  Event.Delete(evVehicleFlipped)
  Event.Delete(eEnterVehicle)
  Event.Delete(eVZSoldierDestroyed)
  L1_2 = Hud.ObjectiveTray
  L3_2 = {}
  L3_2.nSlot = 2
  L1_2.ClearSlot(L1_2, L3_2)
  L1_2 = A0_2.oScoreBoard
  if L1_2 then
    L1_2 = A0_2.oScoreBoard
    L1_2.Close(L1_2)
    A0_2.oScoreBoard = nil
  end
  MrxTaskContract.Cleanup(A0_2)
end

Cleanup = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2, L20_2
  L1_2 = {}
  L1_2[1] = "Fiona-In-Mission-Contract-Dlc02-05"
  L1_2[2] = "Fiona-In-Mission-Contract-Dlc02-06"
  L1_2[3] = "Fiona-In-Mission-Contract-Dlc02-07"
  L2_2 = MrxUtil.GetRandomTableElement(L1_2)
  L6_2 = {}
  L7_2 = Player.GetAnyCharacter()
  L8_2 = Pg.GetGuidByName("check0a")
  L6_2[1] = L7_2
  L6_2[2] = L8_2
  L6_2[3] = "<"
  L6_2[4] = 30
  L6_2[5] = false
  L6_2[6] = false
  L8_2 = {}
  L8_2[1] = L2_2
  A0_2._CreateEvent(A0_2, Event.ObjectProximity, L6_2, MrxVoSequence.Start, L8_2)
  L3_2 = {}
  L3_2[1] = "Fiona-In-Mission-Contract-Dlc02-17"
  L4_2 = MrxUtil.GetRandomTableElement(L3_2)
  L8_2 = {}
  L9_2 = Player.GetAnyCharacter()
  L10_2 = Pg.GetGuidByName("check2")
  L8_2[1] = L9_2
  L8_2[2] = L10_2
  L8_2[3] = "<"
  L8_2[4] = 20
  L8_2[5] = false
  L8_2[6] = false
  L10_2 = {}
  L10_2[1] = L4_2
  A0_2._CreateEvent(A0_2, Event.ObjectProximity, L8_2, MrxVoSequence.Start, L10_2)
  L5_2 = {}
  L5_2[1] = "Fiona-In-Mission-Contract-Dlc02-04"
  L6_2 = MrxUtil.GetRandomTableElement(L5_2)
  L10_2 = {}
  L11_2 = Player.GetAnyCharacter()
  L12_2 = Pg.GetGuidByName("check11")
  L10_2[1] = L11_2
  L10_2[2] = L12_2
  L10_2[3] = "<"
  L10_2[4] = 20
  L10_2[5] = false
  L10_2[6] = false
  L12_2 = {}
  L12_2[1] = L6_2
  A0_2._CreateEvent(A0_2, Event.ObjectProximity, L10_2, MrxVoSequence.Start, L12_2)
  L7_2 = {}
  L7_2[1] = "Fiona-In-Mission-Contract-Dlc02-03"
  L8_2 = MrxUtil.GetRandomTableElement(L7_2)
  L12_2 = {}
  L13_2 = Player.GetAnyCharacter()
  L14_2 = Pg.GetGuidByName("check29")
  L12_2[1] = L13_2
  L12_2[2] = L14_2
  L12_2[3] = "<"
  L12_2[4] = 10
  L12_2[5] = false
  L12_2[6] = false
  L14_2 = {}
  L14_2[1] = L8_2
  A0_2._CreateEvent(A0_2, Event.ObjectProximity, L12_2, MrxVoSequence.Start, L14_2)
  L9_2 = {}
  L9_2[1] = "Fiona-In-Mission-Contract-Dlc01-16"
  L10_2 = MrxUtil.GetRandomTableElement(L9_2)
  L14_2 = {}
  L15_2 = Player.GetAnyCharacter()
  L16_2 = Pg.GetGuidByName("check35")
  L14_2[1] = L15_2
  L14_2[2] = L16_2
  L14_2[3] = "<"
  L14_2[4] = 10
  L14_2[5] = false
  L14_2[6] = false
  L16_2 = {}
  L16_2[1] = L10_2
  A0_2._CreateEvent(A0_2, Event.ObjectProximity, L14_2, MrxVoSequence.Start, L16_2)
end

CheckPointVO = L0_1
