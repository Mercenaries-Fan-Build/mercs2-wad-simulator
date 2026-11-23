local L0_1, L1_1, L2_1, L3_1, L4_1, L5_1, L6_1, L7_1, L8_1, L9_1, L10_1, L11_1, L12_1, L13_1, L14_1, L15_1, L16_1, L17_1, L18_1, L19_1, L20_1, L21_1, L22_1, L23_1, L24_1, L25_1, L26_1, L27_1, L28_1, L29_1, L30_1, L31_1, L32_1, L33_1, L34_1, L35_1, L36_1, L37_1, L38_1, L39_1, L40_1, L41_1, L42_1, L43_1, L44_1, L45_1, L46_1, L47_1, L48_1, L49_1, L50_1
inherit("MrxTaskContract", false)
import("dlc_moonpatrol", false)
import("DLCEscalation", false)
import("MrxFactionManager", false)
import("MrxMusic", false)
import("MrxPmc", false)
import("MrxTaskObjectiveEnterVehicle", false)
import("MrxTaskObjectiveDestroy", false)
import("MrxTimer", false)
import("MrxTutorialManager", false)
import("MrxUtil", false)
import("SpeedTools", true)
import("WifVzBoundary", false)
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
L5_1 = "Alouette3 Attack (VZ) (Driver)"
L3_1[1] = "Heli"
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
L4_1[1] = "Tank"
L4_1[2] = 2
L3_1.tDensity = L4_1
L2_1[1] = L3_1
L3_1 = {}
L4_1 = {}
L4_1.sCommand = "UpdateDensity"
L5_1 = {}
L5_1[1] = "Driving"
L5_1[2] = "Stopped"
L5_1[3] = "Offroad"
L4_1.tSituation = L5_1
L5_1 = {}
L5_1[1] = "Heli"
L5_1[2] = 2
L4_1.tDensity = L5_1
L3_1[1] = L4_1
L4_1 = {}
L5_1 = {}
L5_1.sCommand = "UpdateDensity"
L6_1 = {}
L6_1[1] = "Driving"
L6_1[2] = "Stopped"
L6_1[3] = "Offroad"
L5_1.tSituation = L6_1
L6_1 = {}
L6_1[1] = "Tank"
L6_1[2] = 3
L5_1.tDensity = L6_1
L4_1[1] = L5_1
L5_1 = {}
L6_1 = {}
L6_1.sCommand = "UpdateDensity"
L7_1 = {}
L7_1[1] = "Driving"
L7_1[2] = "Stopped"
L7_1[3] = "Offroad"
L6_1.tSituation = L7_1
L7_1 = {}
L7_1[1] = "Heli"
L7_1[2] = 3
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
L8_1[1] = "Tank"
L8_1[2] = 4
L7_1.tDensity = L8_1
L6_1[1] = L7_1
L7_1 = {}
L8_1 = {}
L8_1.sCommand = "UpdateDensity"
L9_1 = {}
L9_1[1] = "Driving"
L9_1[2] = "Stopped"
L9_1[3] = "Offroad"
L8_1.tSituation = L9_1
L9_1 = {}
L9_1[1] = "Heli"
L9_1[2] = 4
L8_1.tDensity = L9_1
L7_1[1] = L8_1
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
L10_1[2] = 5
L9_1.tDensity = L10_1
L8_1[1] = L9_1
L9_1 = {}
L10_1 = {}
L10_1.sCommand = "UpdateDensity"
L11_1 = {}
L11_1[1] = "Driving"
L11_1[2] = "Stopped"
L11_1[3] = "Offroad"
L10_1.tSituation = L11_1
L11_1 = {}
L11_1[1] = "Heli"
L11_1[2] = 5
L10_1.tDensity = L11_1
L9_1[1] = L10_1
L0_1[1] = L1_1
L0_1[2] = L2_1
L0_1[3] = L3_1
L0_1[4] = L4_1
L0_1[5] = L5_1
L0_1[6] = L6_1
L0_1[7] = L7_1
L0_1[8] = L8_1
L0_1[9] = L9_1
tLocalEscalationTable = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2
  L2_2 = {}
  L2_2[1] = "DLC01_DlcCon001"
  L2_2[2] = "DLC01_SpeedCity"
  L6_2 = {}
  L6_2[1] = A0_2
  MrxLayerManager.Add(L2_2, SetupVehicle, L6_2)
end

LoadAssets = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2
  L2_2 = Object.IsHibernated(Pg.GetGuidByName("BombCar"))
  if L2_2 then
    L2_2 = Event.Create
    L3_2 = Event.ObjectHibernation
    L4_2 = {}
    L4_2[1] = L1_2
    L4_2[2] = "awake"
    
    function L5_2()
      local L0_3, L1_3, L2_3, L3_3, L4_3
      Vehicle.Enter(L1_2, Player.GetPrimaryCharacter(), "d", true)
      A0_2.AssetsLoaded(A0_2)
    end
    
    L2_2(L3_2, L4_2, L5_2)
  else
    Vehicle.Enter(L1_2, Player.GetPrimaryCharacter(), "d", true)
    A0_2.AssetsLoaded(A0_2)
  end
end

SetupVehicle = L0_1
uVehicle = nil
nScore = 0
nTimeBonus = 0
nBonusScore = 0
oMasterTimer = nil
uLastTarget = nil
uTargetMarker = nil
nEscalationIndex = 0
nTargetsComplete = 0
nMinTargetDistance = 0
nMaxTargetDistance = 0
nTargetsUntilEscalation = 0
nTimetoDestroy = 0
nMissedBuildings = 0
nMissedBuildingsAllowed = 0

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2
  MrxTaskContract.Activated(A0_2)
  A0_2._SetCancelMessage(A0_2, "")
  L1_2 = DLCEscalation
  L1_2.tEscalationTable = tLocalEscalationTable
  L1_2 = {}
  L2_2 = {}
  L4_2 = {}
  L5_2 = {}
  L7_2 = "M151 Softtop (VZ) (Driver)"
  L5_2[1] = "Car"
  L5_2[2] = L7_2
  L5_2[3] = 1
  L6_2 = {}
  L8_2 = "Alouette3 Attack (VZ) (Driver)"
  L6_2[1] = "Heli"
  L6_2[2] = L8_2
  L6_2[3] = 1
  L7_2 = {}
  L9_2 = "Scorpion90 (Driver)"
  L7_2[1] = "Tank"
  L7_2[2] = L9_2
  L7_2[3] = 1
  L4_2[1] = L5_2
  L4_2[2] = L6_2
  L4_2[3] = L7_2
  L5_2 = {}
  L6_2 = {}
  L6_2[1] = "Car"
  L6_2[2] = 3
  L7_2 = {}
  L7_2[1] = "Heli"
  L7_2[2] = 1
  L8_2 = {}
  L8_2[1] = "Tank"
  L8_2[2] = 1
  L5_2[1] = L6_2
  L5_2[2] = L7_2
  L5_2[3] = L8_2
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
  L7_2 = {}
  L9_2 = "Alouette3 Attack (VZ) (Driver)"
  L7_2[1] = "Heli"
  L7_2[2] = L9_2
  L7_2[3] = 1
  L8_2 = {}
  L10_2 = "Scorpion90 (Driver)"
  L8_2[1] = "Tank"
  L8_2[2] = L10_2
  L8_2[3] = 1
  L5_2[1] = L6_2
  L5_2[2] = L7_2
  L5_2[3] = L8_2
  L6_2 = {}
  L7_2 = {}
  L7_2[1] = "Car"
  L7_2[2] = 3
  L8_2 = {}
  L8_2[1] = "Heli"
  L8_2[2] = 1
  L9_2 = {}
  L9_2[1] = "Tank"
  L9_2[2] = 1
  L6_2[1] = L7_2
  L6_2[2] = L8_2
  L6_2[3] = L9_2
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
  L1_2[1] = L2_2
  L1_2[2] = L3_2
  L1_2[3] = L4_2
  tLocalPursuitTable = L1_2
  L1_2 = DLCEscalation
  L1_2.tPursuitTable = tLocalPursuitTable
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
  WifVzBoundary.SetupBoundary("Speed Boundary", false)
  uVehicle = Pg.GetGuidByName("BombCar")
  WifMissionFlow.SetGrappleEnabled(false)
  Player.SetVehicleDisguise(false)
  L1_2 = Hud.ResourceCounter
  L3_2 = {}
  L3_2.nDuration = -1
  L1_2.Show(L1_2, L3_2)
  L1_2 = Hud.ResourceCounter
  L3_2 = {}
  L3_2.bSuppressCash = false
  L3_2.bSuppressFuel = true
  L1_2.SetSuppressed(L1_2, L3_2)
  Graphics.Atmosphere.ChangeLineRegionSetting(Pg.GetGuidByName("SpeedAtmo2"), "day")
  SetupMusic()
  L1_2 = MrxTimer
  L3_2 = {}
  L3_2.nStartTime = 0
  L3_2.nStopTime = 3600
  L3_2.nStep = 1
  L3_2.nWarning = 3600
  L3_2.iTray = 0
  oMasterTimer = L1_2.Create(L1_2, L3_2)
  Setup(A0_2)
end

Activated = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2
  Player.SetSeatMovementLocks(Player.GetPrimaryPlayer(), true, false)
  L1_2 = oMasterTimer
  L1_2.Start(L1_2)
  L2_2 = {}
  L2_2.uVehicle = Pg.GetGuidByName("BombCar")
  L2_2.nMaxSpeed = 36.1
  L2_2.nMinSpeedAsPercent = 0.8
  L2_2.nReactorHealth = 40
  L2_2.nHealthLoss = 1
  L2_2.nReactorHealthSlot = 2
  L2_2.nSpeedTraySlot = 3
  SpeedTools.InitializeSpeed(L2_2)
  L2_2 = {}
  L2_2.sHealthPickupLabel = "BarrierSpawn"
  L2_2.nHealthPickups = 50
  L2_2.nHealthPickupMinSpawnDist = 100
  L2_2.nHealthPickupMaxSpawnDist = 400
  L2_2.nHealthPickupSpawnRadius = 90
  L2_2.nHealthPickupOutofRange = 500
  L2_2.bHealthPacksEnabled = true
  L2_2.nSpawnChance = 2
  L2_2.nHealthRestored = 5
  SpeedTools.InitializeRandomHealthPickups(L2_2)
  L2_2 = {}
  L2_2.bEnableBoost = true
  L2_2.nRumbleLength = 0.5
  L2_2.nRumbleIntensity = 0.25
  L2_2.nBoostUseRate = 3
  SpeedTools.SetupBoost(L2_2)
  dlc_moonpatrol.SetImpulse(17)
  dlc_moonpatrol.SetPosition(0.05)
  ResetScore()
  L3_2 = {}
  L3_2[1] = 85
  L5_2 = {}
  L5_2[1] = "Timed"
  evRandomVO = Event.CreatePersistent(Event.TimerRelative, L3_2, PlayRandomVO, L5_2)
  L3_2 = {}
  L3_2[1] = 1
  evTimerScore = Event.CreatePersistent(Event.TimerRelative, L3_2, UpdateScore, {})
  L3_2 = {}
  L3_2[1] = uVehicle
  L5_2 = {}
  L5_2[1] = A0_2
  evVehicleDestroyed = Event.Create(Event.ObjectDeath, L3_2, GameOver, L5_2)
  L1_2 = Event.CreatePersistent
  L2_2 = Event.ObjectDeath
  L3_2 = {}
  L3_2[1] = "Helicopter"
  
  function L4_2()
    local L0_3, L1_3, L2_3, L3_3
    MrxPmc.AddCashQty(5000000, true, "[DLCCon001.UI.AirAssassin]")
  end
  
  evHeliDestroyed = L1_2(L2_2, L3_2, L4_2)
  L1_2 = Event.Create
  L2_2 = Event.TimerRelative
  L3_2 = {}
  L3_2[1] = 2
  
  function L4_2()
    local L0_3, L1_3, L2_3, L3_3, L4_3, L5_3, L6_3, L7_3, L8_3, L9_3, L10_3
    L1_3 = {}
    L1_3[1] = "Fiona-In-Mission-Contract-Dlc01-01"
    L1_3[2] = 0.25
    L1_3[3] = "Fiona-In-Mission-Contract-Dlc01-03"
    L1_3[4] = 0.25
    L1_3[5] = "Fiona-In-Mission-Contract-Dlc01-02"
    L1_3[6] = 0.25
    L1_3[7] = "Fiona-In-Mission-Contract-Dlc01-05"
    L1_3[8] = 0.25
    L1_3[9] = "Fiona-In-Mission-Contract-Dlc01-06"
    MrxVoSequence.Start(L1_3)
    L0_3 = Event.Create
    L1_3 = Event.TimerRelative
    L2_3 = {}
    L2_3[1] = 31
    
    function L3_3()
      local L0_4, L1_4
      PlayRandomVO("BoostDangers")
    end
    
    L0_3(L1_3, L2_3, L3_3)
  end
  
  evVOIntro = L1_2(L2_2, L3_2, L4_2)
  DLCEscalation.StartPursuit(A0_2)
  nEscalationIndex = 2
  nTargetsComplete = 0
  nMinTargetDistance = 400
  nMaxTargetDistance = 500
  nTargetsUntilEscalation = 5
  nTimetoDestroy = 60
  nMissedBuildings = 0
  nMissedBuildingsAllowed = 10
  SetupRandomTargetLocation(A0_2, true)
end

Setup = L0_1

function L0_1()
  local L0_2, L1_2
  nTimeBonus = (nTimeBonus + 20000)
  L0_2 = SpeedTools.IsBoosting()
  if L0_2 then
    nBonusScore = (nBonusScore + 25000)
  end
end

UpdateScore = L0_1

function L0_1()
  local L0_2, L1_2
  nScore = 0
  nTimeBonus = 0
  nBonusScore = 0
end

ResetScore = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2
  L2_2 = table.getn(tTargetLocations)
  L3_2 = 1
  if 0 < L2_2 then
    L4_2 = Pg.GetGuidByName(tTargetLocations[Math.randi(1, L2_2)])
    if A1_2 then
      L4_2 = Pg.GetGuidByName("_caracas_bld_historical04 0x0014f132")
    else
      L5_2 = Object.GetDistanceFrom(uVehicle, L4_2)
      L6_2 = Object.IsAlive(L4_2)
      if L6_2 then
        L6_2 = nMinTargetDistance
        if not (L5_2 < L6_2) then
          L6_2 = nMaxTargetDistance
          if not (L5_2 > L6_2) then
            L6_2 = uLastTarget
            if L4_2 ~= L6_2 then
              goto lbl_54
            end
          end
        end
      end
      SetupRandomTargetLocation(A0_2)
      PlayRandomVO("TargetSwap")
      return
    end
    ::lbl_54::
    L6_2 = A0_2
    L5_2 = A0_2.CreateChild
    L7_2 = {}
    L7_2.sName = "[DLCCon001.UI.DestroyTarget]"
    L7_2.sModuleName = "MrxTaskObjectiveDestroy"
    L7_2.vTgtInclude = L4_2
    L7_2.sDspShortDesc = "[DLCCon001.Objectives.001]"
    L7_2.nDspBlpWldFarDist = 500
    
    function L8_2()
      local L0_3, L1_3, L2_3, L3_3, L4_3, L5_3
      PlayRandomVO("TargetDestroyed")
      L0_3 = uTargetMarker
      if L0_3 then
        Marker.Remove(uTargetMarker)
      end
      nTargetsComplete = (nTargetsComplete + 1)
      L0_3 = nTargetsComplete
      L1_3 = nTargetsUntilEscalation
      if L0_3 == L1_3 then
        DLCEscalation.ParseEscalationTable(nEscalationIndex)
        nEscalationIndex = (nEscalationIndex + 1)
        nTargetsComplete = 0
      end
      MrxPmc.AddCashQty(350000, true, "[DLCCon001.UI.TargetDemolished]")
      nScore = (nScore + 350000)
      Event.Delete(evTargetTimer)
      Event.Delete(evTargetNear)
      L2_3 = {}
      L2_3[1] = 2
      L4_3 = {}
      L4_3[1] = A0_2
      Event.Create(Event.TimerRelative, L2_3, SetupRandomTargetLocation, L4_3)
    end
    
    L7_2.fOnComplete = L8_2
    oTarget = L5_2(L6_2, L7_2)
    L5_2 = Object.GetPosition
    L6_2 = L4_2
    L5_2, L6_2, L7_2 = L5_2(L6_2)
    L10_2 = {}
    L10_2[1] = L4_2
    L12_2 = {}
    L12_2[1] = "DLC_Explosion (Daisy Cutter)"
    L12_2[2] = L5_2
    L12_2[3] = L6_2
    L12_2[4] = L7_2
    evTargetBuildingExplosion = Event.Create(Event.ObjectDeath, L10_2, Pg.Spawn, L12_2)
    L10_2 = {}
    L10_2[1] = L4_2
    L10_2[2] = "awake"
    L12_2 = {}
    L12_2[1] = L4_2
    evTargetAwake = Event.Create(Event.ObjectHibernation, L10_2, CreateDiscMarker, L12_2)
    L10_2 = {}
    L10_2[1] = uVehicle
    L10_2[2] = L4_2
    L10_2[3] = "<"
    L10_2[4] = 200
    L12_2 = {}
    L12_2[1] = "TargetNear"
    evTargetNear = Event.Create(Event.ObjectProximity, L10_2, PlayRandomVO, L12_2)
    L8_2 = Event.Create
    L9_2 = Event.TimerRelative
    L10_2 = {}
    L10_2[1] = nTimetoDestroy
    
    function L11_2()
      local L0_3, L1_3, L2_3, L3_3, L4_3, L5_3
      L0_3 = oTarget
      L0_3.Cancel(L0_3)
      L0_3 = uTargetMarker
      if L0_3 then
        Marker.Remove(uTargetMarker)
      end
      nMissedBuildings = (nMissedBuildings + 1)
      L0_3 = nMissedBuildings
      L1_3 = nMissedBuildingsAllowed
      if L0_3 == L1_3 then
        L0_3 = A0_2
        L0_3._SetCancelMessage(L0_3, "[DLCCon001.UI.BuildingFail]")
        Cancel(A0_2)
        return
      end
      L0_3 = MessageBox
      L0_3.AddMessage(L0_3, "[DLCCon001.UI.TargetChanged]")
      Event.Delete(evTargetBuildingExplosion)
      Event.Delete(evTargetAwake)
      Event.Delete(evTargetNear)
      L2_3 = {}
      L2_3[1] = 5
      L4_3 = {}
      L4_3[1] = A0_2
      evReselectTarget = Event.Create(Event.TimerRelative, L2_3, SetupRandomTargetLocation, L4_3)
    end
    
    evTargetTimer = L8_2(L9_2, L10_2, L11_2)
    uLastTarget = L4_2
  end
end

SetupRandomTargetLocation = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2
  L1_2 = Junk.GetModelBBoxExtents
  L2_2 = A0_2
  L1_2, L2_2, L3_2 = L1_2(L2_2)
  uTargetMarker = Marker.AddDiscDLC(A0_2, (Math.Length(L1_2, 0, L3_2) / 2), 255, 200, 0)
end

CreateDiscMarker = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2
  L1_2 = oTarget
  if L1_2 then
    L1_2 = oTarget
    L1_2.Cancel(L1_2)
  end
  L1_2 = uTargetMarker
  if L1_2 then
    Marker.Remove(uTargetMarker)
  end
  WifVzBoundary.RemoveWorldBoundary()
  WifMissionFlow.SetGrappleEnabled(true)
  Player.SetVehicleDisguise(true)
  Player.SetSeatMovementLocks(Player.GetPrimaryPlayer(), true, true)
  L1_2 = Hud.ResourceCounter
  L3_2 = {}
  L3_2.bSuppressCash = false
  L3_2.bSuppressFuel = false
  L1_2.SetSuppressed(L1_2, L3_2)
  SpeedTools.DeinitSpeed()
  SpeedTools.DeinitRandomHealthPickups()
  L2_2 = {}
  L2_2.bEnableBoost = false
  SpeedTools.SetupBoost(L2_2)
  Event.Delete(evTimerScore)
  Event.Delete(evVOIntro)
  Event.Delete(evVehicleDestroyed)
  Event.Delete(evHeliDestroyed)
  Event.Delete(evTargetTimer)
  Event.Delete(evReselectTarget)
  Event.Delete(evTargetAwake)
  Event.Delete(evRandomVO)
  Event.Delete(evOutofVehicle)
  L1_2 = Hud.ObjectiveTray
  L3_2 = {}
  L3_2.nSlot = 1
  L1_2.ClearSlot(L1_2, L3_2)
  L1_2 = Hud.ObjectiveTray
  L3_2 = {}
  L3_2.nSlot = 2
  L1_2.ClearSlot(L1_2, L3_2)
  L1_2 = Hud.ObjectiveTray
  L3_2 = {}
  L3_2.nSlot = 3
  L1_2.ClearSlot(L1_2, L3_2)
  L1_2 = oMasterTimer
  L1_2.Stop(L1_2)
  L1_2 = oMasterTimer
  L1_2 = L1_2.GetTime(L1_2)
  PlayRandomVO("ContractComplete")
  MrxPmc.AddCashQty((nTimeBonus + nBonusScore))
  L3_2 = MrxPmc.GetCashQty() - ((nScore + nTimeBonus) + nBonusScore)
  L4_2 = ""
  if L3_2 < 0 then
    L4_2 = "-"
  end
  L5_2 = "[DLCCon001.UI.BaseScore] "
  L6_2 = MrxUtil.FormatMoney(nScore)
  L7_2 = "[n]"
  L8_2 = "[Fanfare.Completion.TimeBonus]: "
  L9_2 = MrxUtil.FormatMoney(nTimeBonus)
  L10_2 = "[n]"
  L11_2 = "[DLCCon001.UI.BoostBonus] "
  L12_2 = MrxUtil.FormatMoney(nBonusScore)
  L13_2 = "[n]"
  L14_2 = "[Scoring.Misc]: "
  L17_2 = math.abs
  L18_2 = L3_2
  L17_2, L18_2, L19_2 = L17_2(L18_2)
  L16_2 = MrxUtil.FormatMoney(L17_2, L18_2, L19_2)
  L17_2 = "[n][green][Scoring.Total]: "
  L18_2 = MrxUtil.FormatMoney(MrxPmc.GetCashQty())
  L19_2 = "[n]"
  L5_2 = L5_2 .. L6_2 .. L7_2 .. L8_2 .. L9_2 .. L10_2 .. L11_2 .. L12_2 .. L13_2 .. L14_2 .. L4_2 .. L16_2 .. L17_2 .. L18_2 .. L19_2
  L7_2 = Player.GetPrimaryPlayer()
  L9_2 = {}
  L10_2 = nil
  L11_2 = A0_2.Complete
  L12_2 = {}
  L12_2[1] = A0_2
  L13_2 = nil
  L14_2 = nil
  L15_2 = "center"
  L16_2 = "center"
  L17_2 = true
  L18_2 = nil
  A0_2.oScoreBoard = MrxGui.DisplayDialogBox(L7_2, L5_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2)
  L8_2 = MrxPmc.GetCashQty
  L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2 = L8_2()
  Net.LeaderboardPushScore("DlcCon001", L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2)
  L8_2 = MrxPmc.GetCashQty
  L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2 = L8_2()
  DLC01_MissionHub.SetPrevBest("DlcCon001", L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2)
end

GameOver = L0_1

function L0_1(A0_2)
  local L1_2, L2_2
  A0_2.oScoreBoard = nil
  MrxTaskContract.Complete(A0_2)
end

Complete = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2
  L1_2 = oTarget
  if L1_2 then
    L1_2 = oTarget
    L1_2.Cancel(L1_2)
  end
  L1_2 = uTargetMarker
  if L1_2 then
    Marker.Remove(uTargetMarker)
  end
  L1_2 = oMasterTimer
  if L1_2 then
    L1_2 = oMasterTimer
    L1_2.Stop(L1_2)
  end
  DLCEscalation.ClearPursuit(A0_2)
  WifVzBoundary.RemoveWorldBoundary()
  WifMissionFlow.SetGrappleEnabled(true)
  Player.SetVehicleDisguise(true)
  Player.SetSeatMovementLocks(Player.GetPrimaryPlayer(), true, true)
  L1_2 = Hud.ResourceCounter
  L3_2 = {}
  L3_2.bSuppressCash = false
  L3_2.bSuppressFuel = false
  L1_2.SetSuppressed(L1_2, L3_2)
  SpeedTools.DeinitSpeed()
  SpeedTools.DeinitRandomHealthPickups()
  L2_2 = {}
  L2_2.bEnableBoost = false
  SpeedTools.SetupBoost(L2_2)
  Event.Delete(evTimerScore)
  Event.Delete(evVOIntro)
  Event.Delete(evVehicleDestroyed)
  Event.Delete(evHeliDestroyed)
  Event.Delete(evTargetTimer)
  Event.Delete(evReselectTarget)
  Event.Delete(evTargetAwake)
  Event.Delete(evRandomVO)
  Event.Delete(evOutofVehicle)
  L1_2 = Hud.ObjectiveTray
  L3_2 = {}
  L3_2.nSlot = 1
  L1_2.ClearSlot(L1_2, L3_2)
  L1_2 = Hud.ObjectiveTray
  L3_2 = {}
  L3_2.nSlot = 2
  L1_2.ClearSlot(L1_2, L3_2)
  L1_2 = Hud.ObjectiveTray
  L3_2 = {}
  L3_2.nSlot = 3
  L1_2.ClearSlot(L1_2, L3_2)
  MrxTaskContract.Cancel(A0_2)
end

Cancel = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2
  L1_2 = oTarget
  if L1_2 then
    L1_2 = oTarget
    L1_2.Cancel(L1_2)
  end
  L1_2 = uTargetMarker
  if L1_2 then
    Marker.Remove(uTargetMarker)
  end
  L1_2 = oMasterTimer
  if L1_2 then
    L1_2 = oMasterTimer
    L1_2.Stop(L1_2)
  end
  L1_2 = A0_2.oScoreBoard
  if L1_2 then
    L1_2 = A0_2.oScoreBoard
    L1_2.Close(L1_2)
    A0_2.oScoreBoard = nil
  end
  DLCEscalation.ClearPursuit(A0_2)
  WifVzBoundary.RemoveWorldBoundary()
  WifMissionFlow.SetGrappleEnabled(true)
  Player.SetVehicleDisguise(true)
  Player.SetSeatMovementLocks(Player.GetPrimaryPlayer(), true, true)
  L1_2 = Hud.ResourceCounter
  L3_2 = {}
  L3_2.bSuppressCash = false
  L3_2.bSuppressFuel = false
  L1_2.SetSuppressed(L1_2, L3_2)
  SpeedTools.DeinitSpeed()
  SpeedTools.DeinitRandomHealthPickups()
  L2_2 = {}
  L2_2.bEnableBoost = false
  SpeedTools.SetupBoost(L2_2)
  Event.Delete(evTimerScore)
  Event.Delete(evVOIntro)
  Event.Delete(evVehicleDestroyed)
  Event.Delete(evHeliDestroyed)
  Event.Delete(evTargetTimer)
  Event.Delete(evReselectTarget)
  Event.Delete(evTargetAwake)
  Event.Delete(evRandomVO)
  Event.Delete(evOutofVehicle)
  L1_2 = Hud.ObjectiveTray
  L3_2 = {}
  L3_2.nSlot = 1
  L1_2.ClearSlot(L1_2, L3_2)
  L1_2 = Hud.ObjectiveTray
  L3_2 = {}
  L3_2.nSlot = 2
  L1_2.ClearSlot(L1_2, L3_2)
  L1_2 = Hud.ObjectiveTray
  L3_2 = {}
  L3_2.nSlot = 3
  L1_2.ClearSlot(L1_2, L3_2)
  MrxTaskContract.Cleanup(A0_2)
end

Cleanup = L0_1

function L0_1()
  local L0_2, L1_2
  MrxMusic.PlaySpecialMusic("Dlc_mu_manny")
end

SetupMusic = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2
  L1_2 = pairs
  L2_2 = tRandomVO
  L1_2, L2_2, L3_2 = L1_2(L2_2)
  for L4_2, L5_2 in L1_2, L2_2, L3_2 do
    L6_2 = L5_2.sType
    if L6_2 == A0_2 then
      L7_2 = Math.randi(1, table.getn(L5_2.tVO))
      if A0_2 == "BoostDangers" then
        L9_2 = {}
        L9_2[1] = L5_2.tVO[L7_2]
        L9_2[2] = ShowBoostTutorial
        MrxVoSequence.Start(L9_2)
      else
        MrxVoSequence.Start(L5_2.tVO[L7_2])
      end
    end
  end
end

PlayRandomVO = L0_1

function L0_1()
  local L0_2, L1_2, L2_2, L3_2, L4_2, L5_2
  MrxTutorialManager.ShowMessage("[DLCCon001.UI.Boost]")
  L2_2 = {}
  L2_2[1] = 10
  L4_2 = {}
  L4_2[1] = "[DLCCon001.UI.Jump]"
  Event.Create(Event.TimerRelative, L2_2, MrxTutorialManager.ShowMessage, L4_2)
  L2_2 = {}
  L2_2[1] = 20
  Event.Create(Event.TimerRelative, L2_2, MrxTutorialManager.HideMessage, {})
end

ShowBoostTutorial = L0_1
L0_1 = {}
L0_1[1] = "_caracas_bld_historical04 0x0014f12f"
L0_1[2] = "_caracas_bld_historical04 0x0014f161"
L0_1[3] = "_caracas_bld_historical04 0x0014f4a5"
L0_1[4] = "_caracas_bld_historical05 0x0014f131"
L0_1[5] = "_caracas_bld_historical05 0x0014f133"
L0_1[6] = "_caracas_bld_historical05 0x0014f13a"
L0_1[7] = "_caracas_bld_historical05 0x0014f13b"
L0_1[8] = "_caracas_bld_historical05 0x0014f157"
L0_1[9] = "_caracas_bld_historical05 0x0014f158"
L0_1[10] = "_caracas_bld_historical05 0x0014f168"
L0_1[11] = "_caracas_bld_historical05 0x0014f2f9"
L0_1[12] = "_caracas_bld_historical05 0x0014f320"
L0_1[13] = "_caracas_bld_historical05 0x0014f34e"
L0_1[14] = "_caracas_bld_historical05 0x0014f378"
L0_1[15] = "_caracas_bld_historical05 0x0014f379"
L0_1[16] = "_caracas_bld_historical05 0x0014f48e"
L0_1[17] = "_caracas_bld_historical05 0x0014f48f"
L0_1[18] = "_caracas_bld_historical05 0x0014f4a1"
L0_1[19] = "_caracas_bld_historical05 0x0014f4ba"
L0_1[20] = "_caracas_bld_historical05 0x0014f4bb"
L0_1[21] = "_caracas_bld_historical05 0x0014f4de"
L0_1[22] = "_caracas_bld_historical05 0x00152514"
L0_1[23] = "_caracas_bld_skyscrapercollapsed01 0x0015260f"
L0_1[24] = "_caracas_bld_skyscrapercollapsed01 0x00152706"
L0_1[25] = "_city_bld_corner16x16a 0x0014f17d"
L0_1[26] = "_city_bld_corner16x16a 0x0014f279"
L0_1[27] = "_city_bld_corner16x16a 0x001526bb"
L0_1[28] = "_city_bld_corner16x16a 0x00152707"
L0_1[29] = "_city_bld_corner16x16b 0x0014f0fd"
L0_1[30] = "_city_bld_corner16x16b 0x0014f101"
L0_1[31] = "_city_bld_corner16x16c 0x0014efea"
L0_1[32] = "_city_bld_corner16x16c 0x001525ef"
L0_1[33] = "_city_bld_corner16x32A 0x0014f052"
L0_1[34] = "_city_bld_corner16x32A 0x0014f053"
L0_1[35] = "_city_bld_corner16x32A 0x0014f0b7"
L0_1[36] = "_city_bld_corner16x32A 0x0014f0b9"
L0_1[37] = "_city_bld_corner16x32A 0x0014f13f"
L0_1[38] = "_city_bld_corner16x32A 0x0014f140"
L0_1[39] = "_city_bld_corner16x32A 0x0014f27a"
L0_1[40] = "_city_bld_corner16x32A 0x0014f297"
L0_1[41] = "_city_bld_corner16x32A 0x0014f299"
L0_1[42] = "_city_bld_corner16x32A 0x001526b9"
L0_1[43] = "_city_bld_corner16x32B 0x0014f0fe"
L0_1[44] = "_city_bld_corner16x32B 0x0014f100"
L0_1[45] = "_city_bld_corner16x32B 0x0014f107"
L0_1[46] = "_city_bld_corner16x32B 0x0014f10a"
L0_1[47] = "_city_bld_corner16x32B 0x0014f186"
L0_1[48] = "_city_bld_corner16x32B 0x00152601"
L0_1[49] = "_city_bld_corner32x32A 0x0014f051"
L0_1[50] = "_city_bld_corner32x32A 0x0014f298"
L0_1[51] = "_city_bld_corner32x32A 0x001551dd"
L0_1[52] = "_city_bld_corner32x32B 0x0014f0fc"
L0_1[53] = "_city_bld_corner32x32B 0x0014f0ff"
L0_1[54] = "_city_bld_corner32x32B 0x0014f108"
L0_1[55] = "_city_bld_corner32x32B 0x0014f109"
L0_1[56] = "_city_bld_corner32x32B 0x0014f10e"
L0_1[57] = "_city_bld_corner32x32B 0x0014f10f"
L0_1[58] = "_city_bld_corner32x32D 0x0014eff3"
L0_1[59] = "_city_bld_corner32x32D 0x0014f067"
L0_1[60] = "_city_bld_corner32x32D 0x0014f1d7"
L0_1[61] = "_city_bld_corner32x32D 0x00152642"
L0_1[62] = "_city_bld_corner32x32D 0x001526be"
L0_1[63] = "_city_bld_segment16x16A 0x0014eff4"
L0_1[64] = "_city_bld_segment16x16A 0x0014f1a9"
L0_1[65] = "_city_bld_segment16x16A 0x0014f218"
L0_1[66] = "_city_bld_segment16x16A 0x0014f23a"
L0_1[67] = "_city_bld_segment16x16A 0x001526bc"
L0_1[68] = "_city_bld_segment16x16A 0x001526bd"
L0_1[69] = "_city_bld_segment16x16A 0x001526e3"
L0_1[70] = "_city_bld_segment16x16A 0x001526e4"
L0_1[71] = "_city_bld_segment16x32A 0x0014f00d"
L0_1[72] = "_city_bld_segment16x32A 0x0014f05a"
L0_1[73] = "_city_bld_segment16x32A 0x0014f05c"
L0_1[74] = "_city_bld_segment16x32A 0x0014f063"
L0_1[75] = "_city_bld_segment16x32A 0x0014f065"
L0_1[76] = "_city_bld_segment16x32A 0x0014f094"
L0_1[77] = "_city_bld_segment16x32A 0x0014f095"
L0_1[78] = "_city_bld_segment16x32A 0x0014f0b5"
L0_1[79] = "_city_bld_segment16x32A 0x0014f0b6"
L0_1[80] = "_city_bld_segment16x32A 0x0014f103"
L0_1[81] = "_city_bld_segment16x32A 0x0014f105"
L0_1[82] = "_city_bld_segment16x32A 0x0014f10c"
L0_1[83] = "_city_bld_segment16x32A 0x0014f10d"
L0_1[84] = "_city_bld_segment16x32A 0x0014f187"
L0_1[85] = "_city_bld_segment16x32A 0x0014f1a5"
L0_1[86] = "_city_bld_segment16x32A 0x0014f217"
L0_1[87] = "_city_bld_segment16x32A 0x001525ee"
L0_1[88] = "_city_bld_segment16x32A 0x00152612"
L0_1[89] = "_city_bld_segment16x32A 0x00152613"
L0_1[90] = "_city_bld_segment16x32A 0x00152662"
L0_1[91] = "_city_bld_segment16x32A 0x00152665"
L0_1[92] = "_city_bld_segment16x32A 0x001526ba"
L0_1[93] = "_city_bld_segment32x32A 0x0014eff9"
L0_1[94] = "_city_bld_segment32x32A 0x0014f05b"
L0_1[95] = "_city_bld_segment32x32A 0x0014f064"
L0_1[96] = "_city_bld_segment32x32A 0x0014f093"
L0_1[97] = "_city_bld_segment32x32A 0x0014f0b4"
L0_1[98] = "_city_bld_segment32x32A 0x0014f104"
L0_1[99] = "_city_bld_segment32x32A 0x0014f10b"
L0_1[100] = "_city_bld_segment32x32A 0x00152611"
L0_1[101] = "_city_bld_segment32x32A 0x00152661"
L0_1[102] = "_city_bld_segment32x32A 0x00152664"
L0_1[103] = "_city_bld_segment32x32A 0x001551f0"
L0_1[104] = "_city_bld_skyscraper01 0x0014f066"
L0_1[105] = "_city_bld_skyscraper01 0x0014f13c"
L0_1[106] = "_city_bld_skyscraper01 0x0014f177"
L0_1[107] = "_city_bld_skyscraper01 0x0014f17e"
L0_1[108] = "_city_bld_skyscraper01 0x0014f183"
L0_1[109] = "_city_bld_skyscraper01 0x0014f22b"
L0_1[110] = "_city_bld_skyscraper01 0x0015263f"
L0_1[111] = "_city_bld_skyscraper03 0x0014efff"
L0_1[112] = "_city_bld_skyscraper03 0x0014f0f7"
L0_1[113] = "_city_bld_skyscraper03 0x0014f0fb"
L0_1[114] = "_city_bld_skyscraper03 0x0014f176"
tTargetLocations = L0_1
L0_1 = {}
L1_1 = {}
L1_1.sType = "Timed"
L2_1 = {}
L2_1[1] = "Fiona-In-Mission-Contract-Dlc01-22"
L2_1[2] = "Fiona-In-Mission-Contract-Dlc01-23"
L2_1[3] = "Fiona-In-Mission-Contract-Dlc01-24"
L2_1[4] = "Fiona-In-Mission-Contract-Dlc01-25"
L2_1[5] = "Fiona-In-Mission-Contract-Dlc01-26"
L2_1[6] = "Fiona-In-Mission-Contract-Dlc01-27"
L2_1[7] = "Fiona-In-Mission-Contract-Dlc01-04"
L2_1[8] = "Fiona-In-Mission-Contract-Dlc01-28"
L2_1[9] = "Fiona-In-Mission-Contract-Dlc01-29"
L2_1[10] = "Fiona-In-Mission-Contract-Dlc01-30"
L2_1[11] = "Fiona-In-Mission-Contract-Dlc01-31"
L2_1[12] = "Fiona-In-Mission-Contract-Dlc01-32"
L2_1[13] = "Fiona-In-Mission-Contract-Dlc01-35"
L2_1[14] = "Fiona-In-Mission-Contract-Dlc01-06"
L1_1.tVO = L2_1
L2_1 = {}
L2_1.sType = "ContractComplete"
L3_1 = {}
L3_1[1] = "Fiona-In-Mission-Contract-Chi02-08"
L3_1[2] = "Fiona.vo2fio38"
L3_1[3] = "Fiona.jobscomplete02"
L3_1[4] = "Fiona.Race.DoingGreat04"
L3_1[5] = "Fiona.Wager.Win01"
L3_1[6] = "Fiona.Wager.Winning01"
L3_1[7] = "Fiona.Acheivements.NothingButGoodTime01"
L3_1[8] = "Fiona.Misc.ChickenReaction01"
L3_1[9] = "Fiona.Cam.01"
L3_1[10] = "Fiona.Cam.06"
L3_1[11] = "Fiona.Cam.02"
L3_1[12] = "Fiona-In-Mission-Contract-All01-26"
L3_1[13] = "Fiona-In-Mission-Job-All03-08"
L3_1[14] = "Fiona-In-Mission-Job-All10-10"
L2_1.tVO = L3_1
L3_1 = {}
L3_1.sType = "ContractCanceled"
L4_1 = {}
L4_1[1] = "Fiona-In-Mission-Contract-All04-14"
L3_1.tVO = L4_1
L4_1 = {}
L4_1.sType = "TargetNear"
L5_1 = {}
L5_1[1] = "Fiona-None-Freeplay-None-12"
L5_1[2] = "Fiona-None-Freeplay-None-02"
L5_1[3] = "Fiona-None-Freeplay-None-19"
L5_1[4] = "Fiona-None-Freeplay-None-05"
L5_1[5] = "Fiona-None-Freeplay-None-11"
L5_1[6] = "Fiona-None-Freeplay-None-07"
L4_1.tVO = L5_1
L5_1 = {}
L5_1.sType = "TargetDestroyed"
L6_1 = {}
L6_1[1] = "Fiona-In-Mission-Job-Chi07-06"
L6_1[2] = "Fiona-In-Mission-Job-Chi10-10"
L6_1[3] = "Fiona-In-Mission-Job-Chi10-07"
L6_1[4] = "Fiona-In-Mission-Job-Chi10-08"
L6_1[5] = "Fiona-In-Mission-Job-All05-06"
L5_1.tVO = L6_1
L6_1 = {}
L6_1.sType = "ReactorHealthLow"
L7_1 = {}
L7_1[1] = "Fiona-In-Mission-Contract-Dlc01-17"
L7_1[2] = "Fiona-In-Mission-Contract-Dlc01-18"
L7_1[3] = "Fiona-In-Mission-Contract-Dlc01-19"
L7_1[4] = "Fiona-In-Mission-Contract-Dlc01-20"
L7_1[5] = "Fiona-In-Mission-Contract-Dlc01-21"
L6_1.tVO = L7_1
L7_1 = {}
L7_1.sType = "TargetSwap"
L8_1 = {}
L8_1[1] = "Fiona-In-Mission-Contract-Dlc01-38"
L8_1[2] = "Fiona-In-Mission-Contract-Dlc01-39"
L8_1[3] = "Fiona-In-Mission-Contract-Dlc01-40"
L8_1[4] = "Fiona-In-Mission-Contract-Dlc01-41"
L8_1[5] = "Fiona-In-Mission-Contract-Dlc01-42"
L8_1[6] = "Fiona-In-Mission-Contract-Dlc01-43"
L8_1[7] = "Fiona-In-Mission-Contract-Dlc01-44"
L7_1.tVO = L8_1
L8_1 = {}
L8_1.sType = "BoostDangers"
L9_1 = {}
L9_1[1] = "Fiona-In-Mission-Contract-Dlc01-08"
L9_1[2] = "Fiona-In-Mission-Contract-Dlc01-09"
L9_1[3] = "Fiona-In-Mission-Contract-Dlc01-10"
L9_1[4] = "Fiona-In-Mission-Contract-Dlc01-11"
L9_1[5] = "Fiona-In-Mission-Contract-Dlc01-12"
L8_1.tVO = L9_1
L0_1[1] = L1_1
L0_1[2] = L2_1
L0_1[3] = L3_1
L0_1[4] = L4_1
L0_1[5] = L5_1
L0_1[6] = L6_1
L0_1[7] = L7_1
L0_1[8] = L8_1
tRandomVO = L0_1
