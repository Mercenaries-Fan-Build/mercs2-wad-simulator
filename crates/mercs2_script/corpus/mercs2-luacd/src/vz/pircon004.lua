inherit("MrxTaskContract")
import("MrxLayerManager")
import("DangerousBuilding")
import("MrxGui")
import("MrxUtil")
import("MrxFactionManager")

function LoadAssets(self)
  local tLayersToAdd = {
    "VZ_state_PirCon004"
  }
  MrxLayerManager.Add(tLayersToAdd, AssetsLoaded, {self})
end

function Activated(self)
  MrxTaskContract.Activated(self)
  bFinished = false
  nPlayers = Player.GetCurrentPlayers()
  uPlayerPrim = Player.GetPrimaryCharacter()
  tOrganBoxes = {
    Pg.GetGuidByName("OrganBox_08"),
    Pg.GetGuidByName("OrganBox_09"),
    Pg.GetGuidByName("OrganBox_10"),
    Pg.GetGuidByName("OrganBox_11"),
    Pg.GetGuidByName("OrganBox_12"),
    Pg.GetGuidByName("OrganBox_13"),
    Pg.GetGuidByName("OrganBox_14"),
    Pg.GetGuidByName("OrganBox_15"),
    Pg.GetGuidByName("OrganBox_16"),
    Pg.GetGuidByName("OrganBox_17"),
    Pg.GetGuidByName("OrganBox_18"),
    Pg.GetGuidByName("OrganBox_19"),
    Pg.GetGuidByName("OrganBox_20"),
    Pg.GetGuidByName("OrganBox_21"),
    Pg.GetGuidByName("OrganBox_22"),
    Pg.GetGuidByName("OrganBox_23"),
    Pg.GetGuidByName("OrganBox_24"),
    Pg.GetGuidByName("OrganBox_25"),
    Pg.GetGuidByName("OrganBox_26"),
    Pg.GetGuidByName("OrganBox_27"),
    Pg.GetGuidByName("OrganBox_28"),
    Pg.GetGuidByName("OrganBox_29"),
    Pg.GetGuidByName("OrganBox_31"),
    Pg.GetGuidByName("OrganBox_32"),
    Pg.GetGuidByName("OrganBox_33"),
    Pg.GetGuidByName("OrganBox_34")
  }
  tOrganBoxes02 = {
    Pg.GetGuidByName("OrganBox_02"),
    Pg.GetGuidByName("OrganBox_03"),
    Pg.GetGuidByName("OrganBox_04"),
    Pg.GetGuidByName("OrganBox_05")
  }
  tOrganBoxesB = {
    Pg.GetGuidByName("OrganBox_08b"),
    Pg.GetGuidByName("OrganBox_09b"),
    Pg.GetGuidByName("OrganBox_10b"),
    Pg.GetGuidByName("OrganBox_11b"),
    Pg.GetGuidByName("OrganBox_12b"),
    Pg.GetGuidByName("OrganBox_13b"),
    Pg.GetGuidByName("OrganBox_14b"),
    Pg.GetGuidByName("OrganBox_15b"),
    Pg.GetGuidByName("OrganBox_16b"),
    Pg.GetGuidByName("OrganBox_17b"),
    Pg.GetGuidByName("OrganBox_18b"),
    Pg.GetGuidByName("OrganBox_19b"),
    Pg.GetGuidByName("OrganBox_20b"),
    Pg.GetGuidByName("OrganBox_21b"),
    Pg.GetGuidByName("OrganBox_22b"),
    Pg.GetGuidByName("OrganBox_23b"),
    Pg.GetGuidByName("OrganBox_24b"),
    Pg.GetGuidByName("OrganBox_25b"),
    Pg.GetGuidByName("OrganBox_26b"),
    Pg.GetGuidByName("OrganBox_27b"),
    Pg.GetGuidByName("OrganBox_28b"),
    Pg.GetGuidByName("OrganBox_29b"),
    Pg.GetGuidByName("OrganBox_31b"),
    Pg.GetGuidByName("OrganBox_32b"),
    Pg.GetGuidByName("OrganBox_33b"),
    Pg.GetGuidByName("OrganBox_34b")
  }
  tOrganBoxes02B = {
    Pg.GetGuidByName("OrganBox_02b"),
    Pg.GetGuidByName("OrganBox_03b"),
    Pg.GetGuidByName("OrganBox_04b"),
    Pg.GetGuidByName("OrganBox_05b")
  }
  tSpawnedItems = {}
  nSpawn = 1
  uPickup = Pg.GetGuidByName("PirCon004_OrganTruck")
  uAccepter = Pg.GetGuidByName("PirCon004_DeliveryRecipient")
  tVeh = {uPickup}
  uVZGoal = Pg.GetGuidByName("PirCon004_TruckGoal")
  bFirstWarn = true
  bSecWarn = true
  bFinal = true
  if nPlayers == 2 then
    GetPlayers(self)
  end
  GetCompletions(self)
  self:_CreateEvent(Event.ObjectHibernation, {uPickup, "awake"}, function(self)
    for i, uBox in ipairs(tOrganBoxes) do
      local x, y, z = Object.GetPosition(uBox)
      local nFace = Object.GetYaw(uBox)
      Debug.Printf("-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+ Spawning box #" .. nSpawn .. "!! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-")
      nSpawn = nSpawn + 1
      uSpawn = Pg.Spawn("_global_containertransplant", x, y, z, nFace, true, true)
      table.insert(tSpawnedItems, uSpawn)
    end
  end, {self})
  self:_CreateEvent(Event.TimerRelative, {0.75}, function(self)
    for i, uBox in ipairs(tOrganBoxes02) do
      local x, y, z = Object.GetPosition(uBox)
      local nFace = Object.GetYaw(uBox)
      Debug.Printf("-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+ Spawning box #" .. nSpawn .. "!! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-")
      nSpawn = nSpawn + 1
      uSpawn = Pg.Spawn("_global_containertransplant", x, y, z, nFace, true, true)
      table.insert(tSpawnedItems, uSpawn)
    end
  end, {self})
  if nPlayers == 2 then
    self:_CreateEvent(Event.ObjectHibernation, {uPickupB, "awake"}, function(self)
      Debug.Printf("-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+ Spawning second player assets!! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-")
      for i, uBox in ipairs(tOrganBoxesB) do
        local x, y, z = Object.GetPosition(uBox)
        local nFace = Object.GetYaw(uBox)
        Debug.Printf("-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+ Spawning box #" .. nSpawn .. "!! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-")
        nSpawn = nSpawn + 1
        uSpawn = Pg.Spawn("_global_containertransplant", x, y, z, nFace, true, true)
        table.insert(tSpawnedItems, uSpawn)
      end
    end, {self})
    self:_CreateEvent(Event.TimerRelative, {0.75}, function(self)
      for i, uBox in ipairs(tOrganBoxes02B) do
        local x, y, z = Object.GetPosition(uBox)
        local nFace = Object.GetYaw(uBox)
        Debug.Printf("-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+ Spawning box #" .. nSpawn .. "!! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-")
        nSpawn = nSpawn + 1
        uSpawn = Pg.Spawn("_global_containertransplant", x, y, z, nFace, true, true)
        table.insert(tSpawnedItems, uSpawn)
      end
    end, {self})
  end
  DangerousBuilding.SetRarity("default", "never")
  self:CreateChild({
    sName = "PirCon004: Physics Delivery, Organs for transplant",
    sModuleName = "MrxTaskObjectiveEnterVehicle",
    vTgtInclude = tVeh,
    nQuota = 1,
    sDspShortDesc = "[PirCon004.Objectives.Objective01]",
    tOnComplete = {
      {
        ObjDeliverGoods,
        {self}
      }
    },
    tOnCancel = {
      {
        self.Cancel,
        {self}
      }
    }
  })
  if nPlayers == 1 then
    eTruckDeath = self:_CreateEvent(Event.ObjectDeath, {uPickup}, self.Cancel, {self})
  end
end

function ObjDeliverGoods(self)
  local uPlayer = Player.GetAnyCharacter()
  local nTimeLimit = 7
  local nTimeDecRep = self:GetNumCompletions() * 30 or 0
  local sDeadlineText = "Deadline"
  local uLoc = Pg.GetGuidByName("VZBlock_1")
  local uUPTanker = Pg.GetGuidByName("PirCon004_Tanker(explode)")
  bCoopComplete = false
  nTargetBoxesDelivered = nGoal
  nStartingCargo = table.getn(tSpawnedItems)
  nP1Boxes = 0
  nP2Boxes = 0
  tP1Boxes = {}
  tP2Boxes = {}
  sDetBig = "fx_Explosion_HugeOil"
  sDetMid = "Explosion (AA Detonation)"
  uSmokestack01 = Pg.GetGuidByName("PirCon004_Smokestack(explode)01")
  uSmokestack02 = Pg.GetGuidByName("PirCon004_Smokestack(explode)02")
  uPipes01 = Pg.GetGuidByName("PirCon004_PipeObst01")
  uPipes02 = Pg.GetGuidByName("PirCon004_PipeObst02")
  self:_CreateEvent(Event.ObjectProximity, {
    uPlayer,
    uLoc,
    "<",
    100,
    false,
    false
  }, VZBlock, {self})
  oMainDelivery = self:CreateChild({
    sName = "PirCon004: Deliver goods",
    sModuleName = "MrxTaskObjectiveDeliver",
    vTgtInclude = tVeh,
    nQuota = nPlayers,
    vDestLoc = Pg.GetGuidByName("PirCon004_Dest_Location"),
    fDist = 12,
    bStop = true,
    bXZOnly = true,
    sDspShortDesc = "[PirCon004.Objectives.Objective02]",
    fOnPartComplete = function()
      Debug.Printf("-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+ Calling final co-op objective!" .. tostring(self))
      if nPlayers == 2 and bCoopComplete == false then
        bCoopComplete = true
        DeliveryAccept(self)
      end
    end,
    tOnComplete = {
      {
        Delivered,
        {self}
      }
    },
    tOnCancel = {
      {
        self.Cancel,
        {self}
      }
    }
  })
  oMissionTimer = MrxTimer:Create({
    nStartTime = nTimeLimit * 60 - nTimeDecRep,
    nWarning = 60,
    iTray = 3,
    tDoneCallbacks = {
      {
        Cancel,
        {self}
      }
    }
  })
  oMissionTimer:Start()
  eBoxCheck = self:_CreatePersistentEvent(Event.TimerRelative, {0.5}, CheckOrganBoxesLost, {
    self,
    uPickup,
    "P1"
  })
  eCheckClear = self:_CreateEvent(Event.ObjectProximity, {
    uPickup,
    Pg.GetGuidByName("PirCon004_Dest_Location"),
    "<",
    12,
    false,
    false
  }, Event.Delete, {eBoxCheck})
  if nPlayers == 2 then
    eBoxCheck2 = self:_CreatePersistentEvent(Event.TimerRelative, {0.5}, CheckOrganBoxesLost, {
      self,
      uPickupB,
      "P2"
    })
    eCheckClear2 = self:_CreateEvent(Event.ObjectProximity, {
      uPickupB,
      Pg.GetGuidByName("PirCon004_Dest_Location"),
      "<",
      12,
      false,
      false
    }, Event.Delete, {eBoxCheck2})
  end
  self:_CreateEvent(Event.Boundary, {
    Player.GetAnyCharacter(),
    Pg.GetGuidByName("PirCon004_EndPursuit_region"),
    "enter",
    false
  }, Pg.ClearPursuitLock, {true})
  self:_CreateEvent(Event.ObjectProximity, {
    uPlayer,
    uUPTanker,
    "<",
    50,
    false,
    false
  }, Object.Kill, {uUPTanker})
  ObstacleDetonate(self, uSmokestack01, "PirCon004_explosion01", sDetBig, 100)
  ObstacleDetonate(self, uSmokestack02, "PirCon004_explosion02", sDetBig, 120)
  ObstacleDetonate(self, uPipes01, "PirCon004_explosion03", sDetMid, 175)
  ObstacleDetonate(self, uPipes02, "PirCon004_explosion04", sDetMid, 150)
  self:_CreateEvent(Event.ObjectProximity, {
    uPlayer,
    uVZGoal,
    "<",
    175,
    false,
    false
  }, ObstacleTruck, {self})
  self:_CreateEvent(Event.ObjectDeath, {
    Pg.GetGuidByName("PirCon004_Smokestack(explode)02")
  }, function(self)
    MrxVoSequence.Start({
      "Fiona-In-Mission-MinorContract-Oil05-03"
    })
  end, {self})
  self:_CreateEvent(Event.ObjectDeath, {
    Pg.GetGuidByName("PirCon004_PipeObst01")
  }, function(self)
    MrxVoSequence.Start({
      "Fiona.xfio001"
    })
  end, {self})
  ePursTrigger = self:_CreateEvent(Event.ObjectProximity, {
    Pg.GetGuidByName("PirCon004_OrganTruck"),
    Pg.GetGuidByName("PirCon004_DelStart_loc"),
    ">",
    nPurs,
    false,
    false
  }, StartPursuit, {self})
  ePursWarning = self:_CreateEvent(Event.ObjectProximity, {
    Pg.GetGuidByName("PirCon004_OrganTruck"),
    Pg.GetGuidByName("PirCon004_DelStart_loc"),
    ">",
    nPurs - 50,
    false,
    false
  }, function(self)
    MrxVoSequence.Start({
      "Fiona-In-Mission-MinorContract-Pir04-07"
    })
  end, {self})
  MrxVoSequence.Start({
    "Fiona-Banter-MinorContract-Pir04-01",
    0.5,
    {
      mattias = "Mattias-Banter-MinorContract-Pir04-02",
      jennifer = "Jennifer-Banter-MinorContract-Pir04-03",
      chris = "Chris-Banter-MinorContract-Pir04-04"
    }
  })
  eCueMusic = MrxMusic.PlaySpecialMusic("mu_mission_pircon004_02")
end

function GetPlayers(self)
  local x, y, z = Object.GetPosition(Pg.GetGuidByName("PirCon004_Player2Truck"))
  local nFace = Object.GetYaw(Pg.GetGuidByName("PirCon004_Player2Truck"))
  uPickupB = Pg.Spawn("T300 (empty)", x, y, z, nFace, true, true)
  table.insert(tVeh, uPickupB)
end

function GetCompletions(self)
  local nComp = self:GetNumCompletions()
  if 2 < nComp then
    nComp = 2
  end
  if nComp == 0 then
    nPurs = 800
    nGoal = 3
    nMod = 1
  elseif nComp == 1 then
    nPurs = 550
    nGoal = 5
    nMod = 1.5
  elseif nComp == 2 then
    nPurs = 450
    nGoal = 8
    nMod = 2
  end
end

function StartPursuit(self)
  MrxFactionManager.LockPursuit(Pg.GetGuidByName("VZ"), 1)
end

function SetupChaser(self)
  Debug.Printf(" @@@@@@@@@@@@@@@@@@@@@@@@ Setting up chaser")
  local uVZFilter = ObjectFilter.Create()
  ObjectFilter.SetFilter(uVZFilter, "VZ")
  self:_CreateEvent(Event.ObjectProximity, {
    uVZFilter,
    uPickup,
    "<",
    50,
    false,
    false
  }, VZChaseDelay, {self})
end

function VZChaseDelay(self, tVZcars)
  self:_CreateEvent(Event.TimerRelative, {7}, VZChase, {self, tVZcars})
end

function VZChase(self, tVZcars)
  Debug.Printf(" @@@@@@@@@@@@@@@@@@@@@@@@ Starting Chaser")
  local uVZDriver = Vehicle.GetDriver(tVZcars[1])
  if uVZDriver then
    Ai.Goal({
      AIGuid = uVZDriver,
      Goal = "MoveTo",
      Target = Pg.GetGuidByName("PirCon004_OrganTruck"),
      Force = true,
      Priority = "hiPri",
      Callback = VZChaseTest,
      CallbackData = {self, tVZcars}
    })
    Ai.SetHaste(uVZDriver, 0.7)
    self:_CreateEvent(Event.TimerRelative, {10}, VZChase, {self, tVZcars})
  else
    Debug.Printf("@############## No Driver in that VZ car. ")
    SetupChaser(self)
  end
end

function VZChaseTest(self, tVZcars, Guid, State)
  Debug.Printf("$$$$$$$$$$$$$$$$The VZ Chase is Getting Tested!!")
  if State == 0 then
    Debug.Printf("$$$$$$$$$$$$$VZ didn't make the move")
    self:_CreateEvent(Event.TimerRelative, {10}, VZChase, {self, tVZcars})
  elseif State == 1 then
    Debug.Printf("&&&&&&&&&&&&&&&&&& and he did it!!")
    self:_CreateEvent(Event.TimerRelative, {10}, VZChase, {self, tVZcars})
  end
end

function CheckOrganBoxesLost(self, uVeh, sPlyr)
  local x, y, z = Object.GetHardpointPosition(uVeh, "HP_Truckbed")
  nDistCheck = MrxUtil.GetDistanceBetween(uVeh, uPlayerPrim, false)
  Debug.Printf("\175|_|\175|_|\175|_|\175|_|\175|_|\175|_[ " .. tostring(nDistCheck) .. " meters between host player and " .. tostring(uVeh))
  if uVeh == uPickup and nDistCheck < 65 then
    local x, y, z = Object.GetHardpointPosition(uVeh, "HP_Truckbed")
    local tGoods = Pg.GetObjectsInArea(x, y, z, 1, "Organ Container")
    local nTempGoods = table.getn(tGoods)
    nP1Boxes = nTempGoods
    Debug.Printf("-+-+-+-+-+-+-+-+ Truck #1 has " .. tostring(nP1Boxes) .. " boxes!!")
  elseif uVeh == uPickupB and nDistCheck < 65 then
    local x, y, z = Object.GetHardpointPosition(uVeh, "HP_Truckbed")
    local tGoods = Pg.GetObjectsInArea(x, y, z, 1, "Organ Container")
    local nTempGoods = table.getn(tGoods)
    nP2Boxes = nTempGoods
    Debug.Printf("-+-+-+-+-+-+-+-+ Truck #2 has " .. tostring(nP2Boxes) .. " boxes!!")
  end
  DisplayOrganBoxesLost(self)
end

function FinalOrganBoxCheck(self)
  local x, y, z = Object.GetPosition(Pg.GetGuidByName("PirCon004_Dest_Location"))
  local tLocalGoods = {}
  local tLocalGoods = Pg.GetObjectsInArea(x, y, z, 12, "Organ Container")
  if eBoxCheck then
    Event.Delete(eBoxCheck)
  end
  if eBoxCheck2 then
    Event.Delete(eBoxCheck2)
  end
  nFinalGoods = table.getn(tLocalGoods)
  DisplayOrganBoxesLost(self)
end

function DisplayOrganBoxesLost(self)
  nGoods = nP1Boxes + nP2Boxes
  nOrganBoxes = nGoods
  nOrganBoxMoney = nGoods * 1000
  if nPlayers == 1 then
    sHudText = "[PirCon004.Display.Cargo]" .. MrxUtil.FormatMoney(nOrganBoxMoney)
  else
    sHudText = "[PirCon004.Display.CargoCoop]" .. MrxUtil.FormatMoney(nOrganBoxMoney)
  end
  Hud.ObjectiveTray:SetSlotToText({
    vPlayer = nil,
    nSlot = 1,
    sText = sHudText
  })
  Hud.ObjectiveTray:SetSlotToText({
    vPlayer = nil,
    nSlot = 2,
    sText = "[PirCon004.Display.MinBoxes] " .. MrxUtil.FormatMoney(nTargetBoxesDelivered * 1000)
  })
  if bFirstWarn and nGoods <= 25 then
    bFirstWarn = false
    MrxVoSequence.Start({
      "Fiona-In-Mission-MinorContract-Pir04-01"
    })
  elseif bSecWarn and nGoods <= 5 + nTargetBoxesDelivered then
    bSecWarn = false
    MrxVoSequence.Start({
      "Fiona-In-Mission-MinorContract-Pir04-02"
    })
  end
  if bFinal and nGoods < nTargetBoxesDelivered then
    bFinal = false
    MrxVoSequence.Start({
      "Fiona-In-Mission-MinorContract-Pir04-03",
      {
        self.Cancel,
        {self}
      }
    })
  end
end

function TruckDestroyed(self)
  MrxVoSequence.Start({
    "Fiona-In-Mission-MinorContract-Pir04-03",
    {
      self.Cancel,
      {self}
    }
  })
end

function Delivered(self)
  FinalOrganBoxCheck(self)
  nOrganBoxesDelivered = nFinalGoods
  Debug.Printf("-\175-_-\175-_-\175-_-\175-_-\175-_- nStartingCargo: = " .. nStartingCargo .. "!!")
  Debug.Printf("-\175-_-\175-_-\175-_-\175-_-\175-_- nFinalGoods: = " .. nFinalGoods .. "!!")
  if nFinalGoods == nStartingCargo then
    nBonus = 2000000 * nPlayers
  else
    nBonus = (nFinalGoods - nGoal) * 1000 * nMod
  end
  Debug.Printf("-\175-_-\175-_-\175-_-\175-_-\175-_- nBonus: = " .. nBonus .. "!!")
  ClearTimer(self)
  bFinished = true
  if oTalk and bFinished then
    oTalk:Cancel()
  end
  Hud.ObjectiveTray:SetSlotToText({
    vPlayer = nil,
    nSlot = 3,
    sText = "[PirCon004.Display.Delivered] " .. MrxUtil.FormatMoney(nFinalGoods * 1000)
  })
  Debug.Printf("-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+ Delivered: $" .. nGoods .. "!! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-")
  Debug.Printf("-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+ Required: $" .. nGoal .. "!! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-")
  Debug.Printf("-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+ Current bonus amount: $" .. nBonus .. "!! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-")
  if nPlayers == 2 then
    self:_SetPlayer1Bonus(nBonus / nPlayers)
    self:_SetPlayer2Bonus(nBonus / nPlayers)
    FinalTally(self)
  else
    self:_SetPlayer1Bonus(nBonus)
    FinalTally(self)
  end
end

function DeliveryAccept(self)
  Debug.Printf("-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+ Creating final co-op objective!" .. tostring(self))
  oTalk = self:CreateChild({
    sName = "PirCon004: Accept_the_Delivery",
    sModuleName = "MrxTaskObjectiveAction",
    sActionLabel = "[ContextAction.Talk]",
    vTgtInclude = uAccepter,
    sDspShortDesc = "[PirCon004.Objectives.Objective03]",
    tOnPartComplete = {
      {
        DeliveryDiag,
        {self}
      }
    },
    fOnCancel = function()
      if bFinished == false then
        self:Cancel()
      end
    end
  })
end

selfBackup = 0

function DeliveryDiag(self, uGuid)
  if Net.IsServer() and uGuid == Player.GetSecondaryCharacter() then
    Net.SendCustomEvent("PirCon004", NETEVENT_CLIENTDIAGSHOW, {})
    selfBackup = self
  else
    DisplayDiag()
  end
  MrxVoSequence.Start({
    {
      "OilExec-In-Mission-MinorContract-Pir04-05",
      uAccepter
    }
  })
end

function DisplayDiag()
  tDiagOptions = {
    "[PirCon004.Display.DialogOptA]",
    "[PirCon004.Display.DialogOptB]"
  }
  MrxGui.DisplayDialogBox(Player.GetLocalPlayer(), "[PirCon004.Display.DialogMain]", tDiagOptions, 1, DeliveryDiagAction, {self, tDiagOptions}, 0, 0, "center", "center", false)
end

function DeliveryDiagAction(self, tOptions, nIndex)
  if Net.IsClient() then
    Net.SendCustomEvent("PirCon004", NETEVENT_CLIENTDIAGSELECT, {nIndex})
    return
  end
  Debug.Printf("*=-*-=-*=-*-=-*=-*-=-*=-*-=-* Option #1 is " .. tostring(tOptions[1]))
  Debug.Printf("*=-*-=-*=-*-=-*=-*-=-*=-*-=-* Option #2 is " .. tostring(tOptions[2]))
  Debug.Printf("*=-*-=-*=-*-=-*=-*-=-*=-*-=-* Player has chosen option #" .. tostring(nIndex))
  if nIndex == 1 then
    oMainDelivery.Complete(oMainDelivery)
  elseif nIndex == 2 then
    self:DeliveryAccept()
  end
end

function FinalTally(self)
  if nOrganBoxesDelivered >= nTargetBoxesDelivered and nPlayers == 2 then
    MrxVoSequence.Start({
      "Fiona-In-Mission-MinorContract-Pir04-04",
      {
        self.Complete,
        {self}
      }
    })
  elseif nOrganBoxesDelivered >= nTargetBoxesDelivered then
    MrxVoSequence.Start({
      {
        "OilExec-In-Mission-MinorContract-Pir04-05",
        uAccepter
      },
      "Fiona-In-Mission-MinorContract-Pir04-04",
      {
        self.Complete,
        {self}
      }
    })
  else
    MrxVoSequence.Start({
      "Fiona-In-Mission-MinorContract-Pir04-03",
      {
        self.Cancel,
        {self}
      }
    })
  end
end

function ClearTimer(self)
  oMissionTimer:Stop()
  oMissionTimer = nil
  Hud.ObjectiveTray:SetSlotToText({nSlot = 3, sText = " "})
end

function ObstacleDetonate(self, uBuilding, sLoc, sDet, nDist)
  local x, y, z = Object.GetPosition(Pg.GetGuidByName(sLoc))
  self:_CreateEvent(Event.ObjectProximity, {
    tVeh,
    uBuilding,
    "<",
    nDist,
    false,
    false
  }, function(self)
    Debug.Printf("-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+ Setting up building destruction +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-")
    uBoom = Pg.Spawn(sDet, x, y, z, 0)
    if uBoom then
      Debug.Printf("-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+ Boom!! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-")
    end
    self:_CreateEvent(Event.TimerRelative, {0.1}, function(self)
      uSecBoom = Pg.Spawn(sDet, x, y, z, 0)
      if uSecBoom then
        Debug.Printf("-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+ Second Boom!! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-")
      end
    end, {self})
  end, {self})
end

function ObstacleTruck(self)
  uVZSpawn = Pg.GetGuidByName("PirCon004_TruckSpawn")
  local x, y, z = Object.GetPosition(uVZSpawn)
  local nFace = Object.GetYaw(uVZSpawn)
  uVZTruck = Pg.Spawn("M35 (Guntruck) (VZ) (Driver)", x, y, z, nFace)
  uVZTrucker = Vehicle.GetDriver(uVZTruck)
  table.insert(tSpawnedItems, uVZTruck)
  self:_CreateEvent(Event.ObjectIsReady, {uVZTrucker}, function(self)
    Ai.Goal({
      AIGuid = uVZTrucker,
      Goal = "MoveTo",
      Target = uVZGoal,
      Haste = 0.7,
      Priority = "HiPri",
      Callback = Ai.Role,
      CallbackData = {
        AIGuid = uVZTrucker,
        Role = "Idle",
        Priority = "hiPri"
      }
    })
  end, {self})
end

function Cleanup(self)
  if oMissionTimer then
    ClearTimer(self)
  end
  if eCueMusic then
    MrxMusic.StopSpecialMusic()
  end
  Hud.ObjectiveTray:ClearSlot({vPlayer = nil, nSlot = 1})
  Hud.ObjectiveTray:ClearSlot({vPlayer = nil, nSlot = 2})
  Hud.ObjectiveTray:ClearSlot({vPlayer = nil, nSlot = 3})
  DangerousBuilding.SetRarity("all", "default")
  MrxLayerManager.MarkForRemoval("VZ_state_PirCon004")
  if bAddCoopLayer then
    MrxLayerManager.MarkForRemoval("VZ_state_PirCon004_Coop")
  end
  for i, uItem in ipairs(tSpawnedItems) do
    Object.Remove(uItem)
  end
  for i, uVeh in ipairs(tVeh) do
    Debug.Printf("\183\176\183\176\183\176\183\176\183\176\183\176\183\176\183\176\183\176\183\176\183\176\183\176\183 Setting up removal events for " .. tostring(uVeh))
    eRemovePickup = Event.Create(Event.ObjectHibernation, {uVeh, "hibernated"}, Object.Remove, {uVeh})
  end
  MrxFactionManager.ClearPursuitLock()
  MrxTaskContract.Cleanup(self)
end

NETEVENT_CLIENTDIAGSHOW = 0
NETEVENT_CLIENTDIAGSELECT = 1

function NetEventCallback(nEventType, tArgs)
  if nEventType == NETEVENT_CLIENTDIAGSHOW then
    DisplayDiag()
  elseif nEventType == NETEVENT_CLIENTDIAGSELECT then
    self = selfBackup
    DeliveryDiagAction(self, {}, tArgs[1])
  end
end
