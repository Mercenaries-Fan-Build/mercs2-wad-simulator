inherit("MrxTaskContract")
import("MrxSubtitle")
import("DangerousBuilding")
import("MrxFactionManager")
import("MrxTutorialManager")
NETEVENT_CLIENTSETUP = 0
NETEVENT_CLIENTSETUP2 = 1

function Activated(self)
  MrxTaskContract.Activated(self)
  nGoods = 20
  nGoodCopy = 20
  bClientwasIn = false
  nGoods1 = 20
  nGoods2 = 0
  nCargoValue = 500
  nGoodsDelivered = 0
  bPursuitStarted = false
  nPlayedVO1 = false
  nPlayedVO2 = false
  nPlayedVO3 = false
  nPlayedVO4 = false
  nPlayedVO5 = false
  nPlayedVO6 = false
  tPickups = {"GunPickup"}
  if Player.IsCoopMultiplayer() then
    SetupMPGame(self)
  end
  MrxFactionManager.DisableReporting(true)
  DangerousBuilding.SetRarity("all", "never")
  Vehicle.Usable(Pg.GetGuidByName("stager"), false)
  Net.SendCustomEvent("OilCon020", NETEVENT_CLIENTSETUP, {})
  tInitialVOTable = {
    "Fiona-In-Mission-Contract-Oil020-27",
    {
      mattias = "Mattias-In-Mission-Contract-Oil020-28",
      jennifer = "Jennifer-In-Mission-Contract-Oil020-29",
      chris = "Chris-In-Mission-Contract-Oil020-30"
    },
    "Fiona-In-Mission-Contract-Oil020-31"
  }
  self:CreateChild({
    sName = "OilCon002: deliver guns to the Oil HQ",
    sModuleName = "MrxTaskObjectiveEnterVehicle",
    vTgtInclude = tPickups,
    vVoSeqOnAdd = tInitialVOTable,
    nQuota = 1,
    sDspShortDesc = "[OilCon020.Objectives.001]",
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
  self:_CreateEvent(Event.ObjectProximity, {
    Player.GetAnyCharacter(),
    Pg.GetGuidByName("GunPickup"),
    "<",
    40,
    false,
    false
  }, StartCargoCheck, {self})
  self:_CreateEvent(Event.ObjectProximity, {
    Player.GetAnyCharacter(),
    Pg.GetGuidByName("GunPickup"),
    "<",
    80,
    false,
    false
  }, SpottedTruck, {self})
  self:_CreateEvent(Event.ObjectProximity, {
    Player.GetAnyCharacter(),
    Pg.GetGuidByName("GunPickup"),
    "<",
    120,
    false,
    false
  }, GPSTuteDone, {self})
  self:_CreateEvent(Event.ObjectProximity, {
    Player.GetAnyCharacter(),
    Pg.GetGuidByName("loc_VehDisTalk"),
    "> ",
    85,
    false,
    false
  }, GPSTuteStart, {self})
  self:_CreateEvent(Event.ObjectProximity, {
    Player.GetAnyCharacter(),
    Pg.GetGuidByName("GunPickup"),
    "<",
    7,
    false,
    false
  }, AIReact, {self})
end

function StartCargoCheck(self)
  eCheckGuns = self:_CreatePersistentEvent(Event.TimerRelative, {2}, CheckGoodsLost, {self})
end

function GPSTuteStart(self)
  MrxVoSequence.Start({
    "Fiona-In-Mission-Contract-Oil020-37",
    {
      OpenPDA,
      {self, 1}
    }
  })
end

function OpenPDA(self, uFirstRun)
  if eOpenPDA then
    Event.Delete(eOpenPDA)
  end
  if eFionaFirstOpen then
    Event.Delete(eFionaFirstOpen)
  end
  if eClearBeacon then
    Event.Delete(eClearBeacon)
  end
  if eFionaNO then
    Event.Delete(eFionaNO)
  end
  if eFionaOK then
    Event.Delete(eFionaOK)
  end
  if eSetBeacon then
    Event.Delete(eSetBeacon)
  end
  if eSetAgain then
    Event.Delete(eSetAgain)
  end
  if uFirstRun == 2 then
    DoTheNag(self)
  else
    eGPSnag = self:_CreateEvent(Event.TimerRelative, {5}, DoTheNag, {self})
  end
  MrxTutorialManager.ShowMessage("[OilCon020.Objectives.PDATut]", false, "OilCon020")
  Pda.Map:SetBeaconTutorialMode({bEnable = true})
  eOpenPDA = self:_CreateEvent(Event.ScriptEvent, {
    "PDA Open",
    function(tData)
      Debug.Printf(tostring(tData))
      return Player.GetLocalPlayer() == tData.uPlayer
    end
  }, TuteInPDA, {self})
end

function DoTheNag(self)
  if eGPSnag then
    Event.Delete(eGPSnag)
  end
  local tVo = {
    {
      "Fiona-In-Mission-Contract-Oil020-38"
    },
    {
      "Fiona-In-Mission-Contract-Oil020-39"
    },
    {
      "Fiona-In-Mission-Contract-Oil020-40"
    }
  }
  local sSelectedVo = MrxUtil.GetRandomTableElement(tVo)
  MrxVoSequence.Start({sSelectedVo})
  eGPSnag = self:_CreateEvent(Event.TimerRelative, {9}, DoTheNag, {self})
end

function TuteInPDA(self)
  if eGPSnag then
    Event.Delete(eGPSnag)
  end
  if eClosePDA then
    Event.Delete(eClosePDA)
  end
  if eFionaFirstOpen then
    Event.Delete(eFionaFirstOpen)
  end
  Debug.Printf("PPPPPPPPPPPPPPPPP Inside the PDA (Pretty Darn Awesome)")
  MrxTutorialManager.HideMessage(false, "OilCon020")
  eFionaFirstOpen = self:_CreateEvent(Event.TimerRelative, {0.5, true}, MrxVoSequence.Start, {
    "Fiona-In-Mission-Contract-Oil020-43"
  })
  eSetBeacon = self:_CreateEvent(Event.ScriptEvent, {
    "GPS Beacon Set",
    ValidationFunction
  }, BeaconUsed, {self, tBeaconData})
  eClosePDA = self:_CreateEvent(Event.ScriptEvent, {
    "PDA Close",
    function(tData)
      return uPlayer == tData[1]
    end
  }, OpenPDA, {self, 2})
end

function ValidationFunction(tData)
  return true
end

function BeaconUsed(self, tBeaconData)
  if eFionaFirstOpen then
    Event.Delete(eFionaFirstOpen)
  end
  local nX = tBeaconData.nX
  local nZ = tBeaconData.nY
  if 2675 <= nX and nX <= 2825 and nZ <= -350 and -500 <= nZ then
    if eFionaFirstOpen then
      Event.Delete(eFionaFirstOpen)
    end
    if eFionaNO then
      Event.Delete(eFionaNO)
    end
    if eFionaOK then
      Event.Delete(eFionaOK)
    end
    MrxVoSequence.Start({
      "Fiona-In-Mission-Contract-Oil020-46"
    })
    if eClosePDA then
      Event.Delete(eClosePDA)
    end
    NowClosePDA(self)
  else
    AskToClearBeacon(self)
  end
end

function AskToClearBeacon(self)
  if eFionaOK then
    Event.Delete(eFionaOK)
  end
  if eFionaNO then
    Event.Delete(eFionaNO)
  end
  eClearBeacon = self:_CreateEvent(Event.ScriptEvent, {
    "GPS Beacon Cleared",
    ValidationFunction
  }, BeaconCleared, {self, tBeaconClearData})
  local tVo = {
    {
      "Fiona-In-Mission-Contract-Oil020-44"
    },
    {
      "Fiona-In-Mission-Contract-Oil020-45"
    }
  }
  local sSelectedVo = MrxUtil.GetRandomTableElement(tVo)
  eFionaNO = self:_CreateEvent(Event.TimerRelative, {1.3, true}, MrxVoSequence.Start, {sSelectedVo})
end

function BeaconCleared(self)
  if eFionaNO then
    Event.Delete(eFionaNO)
  end
  if eFionaOK then
    Event.Delete(eFionaOK)
  end
  eSetAgain = self:_CreateEvent(Event.ScriptEvent, {
    "GPS Beacon Set",
    ValidationFunction
  }, BeaconUsed, {self, tBeaconData})
  local tVo = {
    {
      "Fiona-In-Mission-Contract-Oil020-41"
    },
    {
      "Fiona-In-Mission-Contract-Oil020-42"
    },
    {
      "Fiona-In-Mission-Contract-Oil020-43"
    }
  }
  local sSelectedVo = MrxUtil.GetRandomTableElement(tVo)
  eFionaOK = self:_CreateEvent(Event.TimerRelative, {1.5, true}, MrxVoSequence.Start, {sSelectedVo})
end

function NowClosePDA(self)
  if eClosePDA then
    Event.Delete(eClosePDA)
  end
  self:_CreateEvent(Event.ScriptEvent, {
    "PDA Close",
    function(tData)
      return uPlayer == tData[1]
    end
  }, GPSTuteDone, {self})
end

function GPSTuteDone(self)
  if eFionaNO then
    Event.Delete(eFionaNO)
  end
  if eFionaFirstOpen then
    Event.Delete(eFionaFirstOpen)
  end
  if eFionaOK then
    Event.Delete(eFionaOK)
  end
  if eSetBeacon then
    Event.Delete(eSetBeacon)
  end
  if eClearBeacon then
    Event.Delete(eClearBeacon)
  end
  if eClosePDA then
    Event.Delete(eClosePDA)
  end
  if eOpenPDA then
    Event.Delete(eOpenPDA)
  end
  if eGPSnag then
    Event.Delete(eGPSnag)
  end
  if eSetAgain then
    Event.Delete(eSetAgain)
  end
  Pda.Map:SetBeaconTutorialMode({bEnable = false})
end

function VehDisguiseTalk(self)
  Debug.Printf("%%%%%%%%%%%%%%%%%%%  Setting Veh disguise")
  MrxVoSequence.Start({
    "Fiona-In-Mission-Contract-Vz01-215",
    "Fiona-In-Mission-Contract-Vz01-216",
    "Fiona-In-Mission-Contract-Vz01-217"
  })
end

function SetupMPGame(self)
  local x, y, z = Object.GetPosition(Pg.GetGuidByName("GunPickup_2"))
  local nFace = Object.GetYaw(Pg.GetGuidByName("GunPickup_2"))
  uPickupB = Pg.Spawn("El Grande", x, y, z, nFace, true, true)
  table.insert(tPickups, uPickupB)
  tLayersToAdd = {
    "vz_State_OilCon020_MPDeliverables"
  }
  MrxLayerManager.Add(tLayersToAdd)
  bClientwasIn = true
  nGoods2 = 20
  nGoodCopy = 40
  ePursuitStart2 = self:_CreateEvent(Event.ObjectProximity, {
    Pg.GetGuidByName(uPickupB),
    Pg.GetGuidByName("loc_DeliverStart"),
    ">",
    500,
    false,
    false
  }, StartPursuit, {self})
  self:_CreateEvent(Event.ObjectDeath, {
    Pg.GetGuidByName("OilCon020_endTalk")
  }, function(self)
    self:_SetCancelMessage("[PirCon003.Terms.Cancel04]")
    self:Cancel()
  end, {self})
  self:_CreateEvent(Event.ObjectHibernation, {
    Pg.GetGuidByName("OilCon020_endTalk"),
    "awake"
  }, function(self)
    Ai.Goal({
      AIGuid = Pg.GetGuidByName("OilCon020_endTalk"),
      Goal = "Idle",
      Priority = "hiPri"
    })
  end, {self})
end

function SpottedTruck(self)
  Debug.Printf("Spotted truck started %%%%%%%%%%%%%%%%%%%%%%%%%%")
  if Object.IsVisible(Pg.GetGuidByName("GunPickup")) then
    MrxVoSequence.Start({
      "Fiona-In-Mission-Job-Oil00-06"
    })
    Debug.Printf("There it is!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!")
  else
    Debug.Printf("Still no sign of that Truck*************************")
    self:_CreateEvent(Event.TimerRelative, {3}, SpottedTruck, {self})
  end
end

function FionaNag(self)
  MrxVoSequence.Start({
    "Fiona-In-Mission-Contract-Oil020-32",
    "Fiona-In-Mission-Job-Oil00-06"
  })
end

function AIReact(self)
  eFionaNag = self:_CreateEvent(Event.ObjectProximity, {
    Player.GetAllCharacters(),
    Pg.GetGuidByName("GunPickup"),
    ">",
    40,
    false,
    false
  }, FionaNag, {self})
  Ai.Goal({
    AIGuid = Pg.GetGuidByName("BoatGunnerMan"),
    Goal = "Enter",
    Target = Pg.GetGuidByName("GunningBoat"),
    Role = "gunner",
    Priority = "hiPri"
  })
  StageJeep(self)
end

function ObjDeliverGoods(self)
  if eFionaNag then
    Event.Delete(eFionaNag)
  end
  self:_CreateEvent(Event.TimerRelative, {5}, MrxVoSequence.Start, {
    "Fiona-In-Mission-Contract-Oil020-01"
  })
  MrxVoSequence.Start({
    "VZSoldier-In-Mission-Contract-Oil020-17"
  })
  MrxMusic.PlaySpecialMusic("mu_fac_oc_kickass_01")
  for i = 1, 4 do
    local uPlayed = Player.GetAnyCharacter()
    local nTruck = i
    local uLoc = Pg.GetGuidByName("CartelBlock_" .. nTruck)
    self:_CreateEvent(Event.ObjectProximity, {
      uPlayed,
      uLoc,
      "<",
      120,
      false,
      false
    }, CartelBlock, {self, nTruck})
  end
  DisplayGoodsLost(self)
  self:CreateChild({
    sName = "OilCon002: Deliver goods",
    sModuleName = "MrxTaskObjectiveDeliver",
    vDestLoc = Pg.GetGuidByName("loc_GunDrop"),
    vTgtInclude = tPickups,
    nQuota = 1,
    fDist = 15,
    bStop = true,
    bXZOnly = true,
    sDspShortDesc = "[OilCon020.Objectives.002]",
    fOnComplete = function()
      if bClientwasIn then
        ActivateDelivered(self)
      else
        CountDelivered(self)
      end
    end,
    fOnCancel = function()
      nPlayedVO5 = true
      self:_SetCancelMessage("[PirCon003.Terms.Cancel03]")
      MrxVoSequence.Start({
        "Fiona-In-Mission-Contract-Oil020-07",
        {
          self.Cancel,
          {self}
        }
      })
    end
  })
  self:_CreateEvent(Event.TimerRelative, {18}, NowDriveVO, {self})
  ePursuitStart = self:_CreateEvent(Event.ObjectProximity, {
    Pg.GetGuidByName("GunPickup"),
    Pg.GetGuidByName("loc_DeliverStart"),
    ">",
    550,
    false,
    false
  }, StartPursuit, {self})
end

function NowDriveVO(self)
  if Object.IsPlayerControlled(Pg.GetGuidByName("GunPickup")) then
    MrxVoSequence.Start({
      "Fiona-In-Mission-Job-Oil00-07"
    })
  end
end

function StageJeep(self)
  Vehicle.Usable(Pg.GetGuidByName("stager"), true)
  local uStageJeep = Pg.GetGuidByName("stager")
  if uStageJeep then
    uStageJeepDriver = Vehicle.GetDriver(uStageJeep)
    if uStageJeepDriver and not Object.IsPlayerControlled(uStageJeep) then
      Ai.Goal({
        AIGuid = uStageJeepDriver,
        Goal = "PathMove",
        Target = Pg.GetGuidByName("Pa_stager"),
        Priority = "hiPri"
      })
    else
      x, y, z = Object.GetPosition(Pg.GetGuidByName("stager"))
      tStillAlive = Pg.FastCollectHumans(x, y, z, 10, "VZ")
      nStillAlive = table.getn(tStillAlive)
      if nStillAlive > 0 then
        Ai.Goal({
          AIGuid = tStillAlive[1],
          Goal = "Enter",
          Target = Pg.GetGuidByName("stager"),
          Priority = "hiPri",
          Callback = StageJeep,
          CallbackData = {self}
        })
      end
    end
  end
end

function StartPursuit(self)
  if not bPursuitStarted then
    bPursuitStarted = true
    MrxVoSequence.Start({
      "Fiona-In-Mission-Contract-Oil020-02"
    })
    self:_CreateEvent(Event.TimerRelative, {10}, MrxVoSequence.Start, {
      "Fiona-In-Mission-Contract-Oil020-11"
    })
    local tPursuitTable = {
      {
        "Driving",
        {
          {
            "Car",
            "M151 .50Cal (VZ) (DriverGunner)",
            1
          }
        },
        {
          {"Car", 3}
        }
      },
      {
        "Stopped",
        {
          {
            "Car",
            "M151 .50Cal (VZ) (DriverGunner)",
            1
          }
        },
        {
          {"Car", 4}
        }
      },
      {
        "Offroad",
        {
          {
            "Car",
            "M151 .50Cal (VZ) (DriverGunner)",
            1
          }
        },
        {
          {"Car", 2}
        }
      }
    }
    MrxFactionManager.SetCustomPursuit(Pg.GetGuidByName("VZ"), -1, tPursuitTable)
    self:_CreateEvent(Event.Boundary, {
      Player.GetAnyCharacter(),
      Pg.GetGuidByName("Reg_Oil020_EndPursuit"),
      "enter",
      false
    }, StopPursuit, {self})
  end
end

function StopPursuit(self)
  bPursuitStarted = false
  Debug.Printf("SSSSSSSSSSSSSSSStop stop Pursuit ")
  MrxFactionManager.ClearCustomPursuit()
  MrxVoSequence.Start({
    "Fiona-In-Mission-Contract-Oil020-14"
  })
end

function CartelBlock(self, nTruck)
  if nTruck == 1 then
    MrxVoSequence.Start({
      "Fiona-In-Mission-Contract-Oil020-18"
    })
  elseif nTruck == 3 then
    MrxVoSequence.Start({
      "Fiona-In-Mission-Contract-Oil020-13"
    })
  elseif nTruck == 5 then
    MrxVoSequence.Start({
      "Fiona-In-Mission-Contract-Oil020-12"
    })
  end
  Debug.Printf("*$#*#$@$#@$@#$$$$##########BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBblocking the road! ")
  local uBlocker = Pg.GetGuidByName("CartelBlock_" .. nTruck)
  if uBlocker then
    local uBlockerDriver = Vehicle.GetDriver(uBlocker)
    if uBlockerDriver and not Object.IsPlayerControlled(uBlocker) then
      local uBlockParam = {
        AIGuid = uBlockerDriver,
        Goal = "PathMove",
        Target = Pg.GetGuidByName("Pa_CartelBlock_" .. nTruck),
        Priority = "hiPri",
        Callback = RoadBlockStop,
        CallbackData = {self, nTruck}
      }
      self:_CreateEvent(Event.TimerRelative, {2}, Ai.Goal, {uBlockParam})
    end
  end
end

function RoadBlockStop(self, nTruck, Guid, State)
  if State == 0 then
    CartelBlock(self, nTruck)
  else
    Debug.Printf("####################   I should be stopping")
    Ai.Goal({
      AIGuid = Guid,
      Goal = "Stop",
      Priority = "hiPri",
      Callback = NowBlockerExit,
      CallbackData = {self}
    })
  end
end

function NowBlockerExit(self, Guid, State)
  Ai.Goal({
    AIGuid = Guid,
    Goal = "Exit",
    Priority = "hiPri"
  })
end

function BlockerChase(self, nTruck, Guid, State)
  if State == 0 then
    CartelBlock(self, nTruck)
  else
    local uCartelDriver = Guid
    if uCartelDriver then
      Ai.Goal({
        AIGuid = uCartelDriver,
        Goal = "MoveTo",
        Target = Pg.GetGuidByName("GunPickup"),
        Force = true,
        Priority = "hiPri",
        Callback = BlockerChase,
        CallbackData = {self}
      })
      Ai.SetHaste(uCartelDriver, 1)
      self:_CreateEvent(Event.TimerRelative, {4}, BlockerChase, {self, Guid})
    end
  end
end

function CheckGoodsLost(self)
  local nGoodCopy = nGoods
  local uMainTruck = Pg.GetGuidByName("GunPickup")
  if uMainTruck then
    if Object.IsAlive(uMainTruck) then
      if not MrxUtil.TestDistanceToAllPlayers(uMainTruck, 30, false, true) then
        self:_CreateEvent(Event.TimerRelative, {1}, function(self)
          if not MrxUtil.TestDistanceToAllPlayers(uMainTruck, 30, false, true) then
            local x, y, z = Object.GetHardpointPosition(uMainTruck, "HP_Truckbed")
            if x and y and z then
              local tGoods = Pg.GetObjectsInArea(x, y, z, 1, "OilCon020gun")
              nGoods1 = table.getn(tGoods)
            end
          end
        end, {self})
      end
    else
      nGoods1 = 0
    end
  end
  if bClientwasIn then
    local uBonusTruck = uPickupB
    if uBonusTruck then
      if Object.IsAlive(uBonusTruck) then
        if not MrxUtil.TestDistanceToAllPlayers(uBonusTruck, 30, false, true) then
          self:_CreateEvent(Event.TimerRelative, {1}, function(self)
            if not MrxUtil.TestDistanceToAllPlayers(uBonusTruck, 30, false, true) then
              local x, y, z = Object.GetHardpointPosition(uBonusTruck, "HP_Truckbed")
              if x and y and z then
                local tGoods2 = Pg.GetObjectsInArea(x, y, z, 1, "OilCon020gun")
                nGoods2 = table.getn(tGoods2)
              end
            end
          end, {self})
        end
      else
        nGoods2 = 0
      end
    end
  end
  nGoods = nGoods1 + nGoods2
  if nGoodCopy > nGoods then
    DisplayGoodsLost(self)
  end
end

function DisplayGoodsLost(self)
  nGunMoney = nGoods * nCargoValue
  sHudText = [[
[OilCon020.Objectives.hudCash]: 
 ]] .. MrxUtil.FormatMoney(nGunMoney)
  Hud.ObjectiveTray:SetSlotToText({
    vPlayer = nil,
    nSlot = 1,
    sText = sHudText
  })
  if nGoods == 35 and not nPlayedVO1 then
    nPlayedVO1 = true
    self:_CreateEvent(Event.TimerRelative, {1}, self._PlayVo, {
      self,
      0,
      "Fiona-In-Mission-Job-Oil00-08"
    })
  elseif nGoods == 25 and not nPlayedVO2 then
    nPlayedVO2 = true
    self:_CreateEvent(Event.TimerRelative, {1}, self._PlayVo, {
      self,
      0,
      "Fiona-In-Mission-Job-Oil00-09"
    })
  elseif nGoods == 18 and not nPlayedVO6 then
    nPlayedVO6 = true
    MrxVoSequence.Start({
      "Fiona-In-Mission-Contract-Oil020-34"
    })
  elseif nGoods == 12 and not nPlayedVO3 then
    nPlayedVO3 = true
    self:_CreateEvent(Event.TimerRelative, {1}, self._PlayVo, {
      self,
      0,
      "Fiona-In-Mission-Job-Oil00-11"
    })
  elseif nGoods == 9 and not nPlayedVO4 then
    nPlayedVO4 = true
    MrxVoSequence.Start({
      "Fiona-In-Mission-Contract-Oil020-06"
    })
  end
  if nGoods == 0 and not nPlayedVO5 then
    nPlayedVO5 = true
    self:_SetCancelMessage("[OilCon020.Terms.Cancel02]")
    MrxVoSequence.Start({
      "Fiona-In-Mission-Job-Oil00-12"
    })
    self:_CreateEvent(Event.TimerRelative, {3}, self.Cancel, {self})
  end
end

function ActivateDelivered(self)
  oEndTalk = self:CreateChild({
    sName = "OilCon020 End",
    sModuleName = "MrxTaskObjectiveAction",
    vTgtInclude = "OilCon020_endTalk",
    sDspShortDesc = "[OilCon020.Objectives.003]",
    tOnComplete = {
      {
        CountDelivered,
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
end

function CountDelivered(self)
  if eCheckGuns then
    Event.Delete(eCheckGuns)
  end
  nPlayedVO1 = true
  nPlayedVO2 = true
  nPlayedVO3 = true
  nPlayedVO4 = true
  nPlayedVO5 = true
  nPlayedVO6 = true
  local x, y, z = Object.GetPosition(Pg.GetGuidByName("loc_GunDrop"))
  local tGoodsDelivered = {}
  local tGoodsDelivered = Pg.GetObjectsInArea(x, y, z, 25, "OilCon020gun")
  nGoodsDelivered = table.getn(tGoodsDelivered)
  nTotalcash = nGoodsDelivered * nCargoValue
  if Net.IsActive() then
    local nMPTotalcash = nTotalcash / 2
    self:_SetPlayer1Bonus(nMPTotalcash)
    self:_SetPlayer2Bonus(nMPTotalcash)
  else
    self:_SetPlayer1Bonus(nTotalcash)
  end
  self:_CreateEvent(Event.TimerRelative, {2}, IntercomResponse, {self})
  local uContact = Pg.GetGuidByName("OilCon020_takesTruck")
  if uContact and Object.IsAlive(uContact) and not Object.IsPlayerControlled(Pg.GetGuidByName("GunPickup")) then
    Ai.Goal({
      AIGuid = uContact,
      Goal = "Enter",
      Target = Pg.GetGuidByName("GunPickup"),
      Role = "driver",
      Force = true,
      Priority = "hiPri",
      Callback = OCtakesTruck,
      CallbackData = {self}
    })
  end
end

function OCtakesTruck(self, uDriver, State)
  if State == 1 and not Object.IsPlayerControlled(Pg.GetGuidByName("GunPickup")) then
    Ai.Goal({
      AIGuid = uDriver,
      Goal = "PathMove",
      Target = Pg.GetGuidByName("Pa_OCTakes"),
      Priority = "hiPri"
    })
  end
end

function IntercomResponse(self)
  if nGoodsDelivered >= 10 then
    MrxVoSequence.Start({
      "OCMerc-In-Mission-Contract-Oil020-03",
      "Fiona-In-Mission-Job-Oil00-13",
      {
        self.Complete,
        {self}
      }
    })
  elseif nGoodsDelivered >= 1 then
    MrxVoSequence.Start({
      "OCMerc-In-Mission-Contract-Oil020-04",
      "Fiona-In-Mission-Job-Oil00-13",
      {
        self.Complete,
        {self}
      }
    })
  else
    self:_SetCancelMessage("[OilCon020.Terms.Cancel02]")
    MrxVoSequence.Start({
      "Fiona-In-Mission-Job-Oil00-12",
      {
        self.Cancel,
        {self}
      }
    })
  end
end

function OnPlayerJoined(self, iPlayerId, uPlayerGuid, uCharGuid)
  Net.SendCustomEvent("OilCon020", NETEVENT_CLIENTSETUP, {})
end

function NetEventCallback(nEventId, tArgs)
  if nEventId == NETEVENT_CLIENTSETUP then
  end
  if nEventId == NETEVENT_CLIENTSETUP2 then
  end
end

function Cleanup(self)
  Hud.ObjectiveTray:ClearSlot({vPlayer = nil, nSlot = 1})
  Net.SendCustomEvent("OilCon020", NETEVENT_CLIENTSETUP2, {})
  local tPlayers = Player.GetAllPlayers()
  for i, uPlayerGuid in ipairs(tPlayers) do
    local uCharacter = Player.GetCharacter(uPlayerGuid)
    Human.ForceExitSeatNoSnap(uCharacter)
  end
  MrxFactionManager.ClearCustomPursuit()
  MrxFactionManager.DisableReporting(false)
  MrxMusic.StopSpecialMusic("none")
  MrxLayerManager.Remove("vz_State_OilCon020_Deliveribles")
  if bClientwasIn then
    MrxLayerManager.Remove("vz_State_OilCon020_MPDeliverables")
  end
  MrxTutorialManager.HideMessage(self)
  GPSTuteDone(self)
  MrxTaskContract.Cleanup(self)
end
