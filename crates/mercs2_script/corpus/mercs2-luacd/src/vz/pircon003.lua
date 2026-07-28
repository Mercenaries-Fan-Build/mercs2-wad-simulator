inherit("MrxTaskContract")
import("MrxSubtitle")
import("MrxTimer")
import("MrxFactionManager")

function Activated(self)
  MrxTaskContract.Activated(self)
  nPlayed1 = 0
  nPlayed2 = 0
  gbLostTooManyBirds = nil
  nParrotLostLine = 10
  nGoods = 42
  nGoodCopy = 42
  bClientwasIn = false
  nGoods1 = 42
  nGoods2 = 0
  nTruckHealth = 100
  ParrotChat = 0
  bParrotVOon = false
  nCargoValue = 1900
  nGoodsDelivered = 0
  tInitialVOTable = {}
  tPickups = {
    "ParrotPickup"
  }
  if Player.IsCoopMultiplayer() then
    SetupMPGame(self)
  end
  local nCompletions = self:GetNumCompletions()
  if 2 <= nCompletions then
    nDifficult = 2
    nRequired = 25
    nRequiredCash = nRequired * nCargoValue
    tInitialVOTable = {
      "Fiona-In-Mission-MinorContract-Pir03-40",
      "Fiona-In-Mission-MinorContract-Pir03-41",
      "Fiona-In-Mission-MinorContract-Pir03-37"
    }
  elseif nCompletions == 1 then
    nDifficult = 1
    nRequired = 15
    nRequiredCash = nRequired * nCargoValue
    tInitialVOTable = {
      "Fiona-In-Mission-MinorContract-Pir03-38",
      "Fiona-In-Mission-MinorContract-Pir03-39",
      "Fiona-In-Mission-MinorContract-Pir03-36"
    }
  else
    nDifficult = 0
    nRequired = 7
    nRequiredCash = nRequired * nCargoValue
  end
  self:CreateChild({
    sName = "PirCon003gotoVehicle",
    sModuleName = "MrxTaskObjectiveEnterVehicle",
    vTgtInclude = tPickups,
    nQuota = 1,
    vVoSeqOnAdd = tInitialVOTable,
    sDspShortDesc = "[PirCon003.Objectives.001]",
    tOnComplete = {
      {
        DeliverTruck,
        {self}
      }
    },
    fOnCancel = function()
      bParrotVOon = false
      self:_SetCancelMessage("[PirCon003.Terms.Cancel03]")
      MrxMusic.StopSpecialMusic("none")
      MrxVoSequence.Start({
        "Fiona-In-Mission-Contract-Oil020-07",
        {
          self.Cancel,
          {self}
        }
      })
    end
  })
  self:_CreateEvent(Event.ObjectProximity, {
    Player.GetAnyCharacter(),
    Pg.GetGuidByName("ParrotPickup"),
    "<",
    10,
    false,
    false
  }, BirdTalk, {self})
  self:_CreateEvent(Event.TimerRelative, {3}, StartCheckingBirds, {self})
end

function StartCheckingBirds(self)
  self:_CreatePersistentEvent(Event.TimerRelative, {1.5}, CheckGoodsLost, {self})
end

function BirdTalk(self)
  local nCompletions = self:GetNumCompletions()
  if nCompletions == 2 or nCompletions == 6 then
    self:_CreateEvent(Event.TimerRelative, {9}, VO.CueWithoutSubtitles, {
      0,
      "Parrot-In-Mission-MinorContract-Pir03-07"
    })
    self:_CreateEvent(Event.TimerRelative, {10}, function(self)
      MrxVoSequence.Start({
        "Fiona-In-Mission-MinorContract-Pir03-28",
        {
          mattias = "Mattias-In-Mission-MinorContract-Pir03-30",
          jennifer = "jennifer-In-Mission-MinorContract-Pir03-31",
          chris = "chris-In-Mission-MinorContract-Pir03-32"
        },
        {
          ParrotVO,
          {self}
        }
      })
    end, {self})
  elseif nCompletions == 1 or nCompletions == 4 then
    self:_CreateEvent(Event.TimerRelative, {9}, VO.CueWithoutSubtitles, {
      0,
      "Parrot-In-Mission-MinorContract-Pir03-07"
    })
    self:_CreateEvent(Event.TimerRelative, {10}, function(self)
      MrxVoSequence.Start({
        "Fiona-In-Mission-MinorContract-Pir03-29",
        {
          mattias = "Mattias-In-Mission-MinorContract-Pir03-33",
          jennifer = "jennifer-In-Mission-MinorContract-Pir03-34",
          chris = "chris-In-Mission-MinorContract-Pir03-35"
        },
        {
          ParrotVO,
          {self}
        }
      })
    end, {self})
  else
    VO.CueWithoutSubtitles(0, "Parrot-In-Mission-MinorContract-Pir03-07")
    self:_CreateEvent(Event.TimerRelative, {1}, VO.CueWithoutSubtitles, {
      0,
      "Parrot-In-Mission-MinorContract-Pir03-09"
    })
    self:_CreateEvent(Event.TimerRelative, {2}, function(self)
      MrxVoSequence.Start({
        "Fiona-Banter-MinorContract-Pir02-01",
        {
          mattias = "mattias-Banter-MinorContract-Pir02-02",
          jennifer = "jennifer-Banter-MinorContract-Pir02-03",
          chris = "chris-Banter-MinorContract-Pir02-04"
        },
        "Fiona-Banter-MinorContract-Pir02-05",
        {
          mattias = "mattias-Banter-MinorContract-Pir02-06",
          jennifer = "jennifer-Banter-MinorContract-Pir02-07",
          chris = "chris-Banter-MinorContract-Pir02-08"
        },
        "Fiona-Banter-MinorContract-Pir02-13",
        {
          mattias = "mattias-Banter-MinorContract-Pir02-14",
          jennifer = "jennifer-Banter-MinorContract-Pir02-15",
          chris = "chris-Banter-MinorContract-Pir02-16"
        },
        "Fiona-Banter-MinorContract-Pir02-17",
        {
          ParrotVO,
          {self}
        }
      })
    end, {self})
  end
end

function DeliverTruck(self)
  MrxMusic.PlaySpecialMusic("mu_mission_pircon003_02")
  local nCompletions = self:GetNumCompletions()
  tPursuitTable = {}
  if 2 <= nCompletions then
    Debug.Printf("+++++++++++++++++++++++++++  Final difficulty!!!!!! ! !1 !")
    sScaleObj = "[PirCon003.Objectives.Hard]"
  elseif nCompletions == 1 then
    Debug.Printf("+++++++++++++++++++++++++++ This is the Second run 222222222222222222")
    sScaleObj = "[PirCon003.Objectives.Med]"
  else
    sScaleObj = "[PirCon003.Objectives.Easy]"
  end
  sHText = "[PirCon003.Objectives.Req]" .. MrxUtil.FormatMoney(nRequiredCash) .. " [PirCon003.Objectives.ReqB]"
  Hud.ObjectiveTray:SetSlotToText({
    vPlayer = nil,
    nSlot = 2,
    sText = sHText
  })
  DisplayGoodsLost(self)
  self:CreateChild({
    sName = "PirCon003: Deliver goods",
    sModuleName = "MrxTaskObjectiveDeliver",
    vTgtInclude = tPickups,
    nQuota = 1,
    vDestLoc = Pg.GetGuidByName("loc_DeliverySpot"),
    fDist = 10,
    sDspShortDesc = sScaleObj,
    bStop = true,
    bXZOnly = false,
    fOnComplete = function()
      if bClientwasIn then
        ActivateDelivered(self)
      else
        CountDelivered(self)
      end
    end,
    fOnCancel = function()
      bParrotVOon = false
      self:_SetCancelMessage("[PirCon003.Terms.Cancel03]")
      MrxMusic.StopSpecialMusic("none")
      MrxVoSequence.Start({
        "Fiona-In-Mission-Contract-Oil020-07",
        {
          self.Cancel,
          {self}
        }
      })
    end
  })
  self:_CreateEvent(Event.ObjectProximity, {
    Pg.GetGuidByName("ParrotPickup"),
    Pg.GetGuidByName("loc_vzAttack"),
    ">",
    150,
    false,
    false
  }, DaCustomPursuit, {self, nDifficult})
  self:_CreateEvent(Event.ObjectProximity, {
    Player.GetAnyCharacter(),
    Pg.GetGuidByName("loc_DeliverySpot_end"),
    "<",
    250,
    false,
    false
  }, MrxFactionManager.ClearCustomPursuit, {})
  self:_CreateEvent(Event.ObjectProximity, {
    Player.GetAnyCharacter(),
    Pg.GetGuidByName("loc_CliffyA"),
    "<",
    50,
    false,
    false
  }, CliffyA, {self})
end

function ParrotVO(self)
  bParrotVOon = true
  self:_CreateEvent(Event.TimerRelative, {4}, SpeedVO, {self})
  self:_CreateEvent(Event.TimerRelative, {6}, DamageVO, {self})
end

function DamageVO(self)
  local nOldTruckHealth = nTruckHealth
  nTruckHealth = Object.GetHealth(Pg.GetGuidByName("ParrotPickup"))
  if nOldTruckHealth > nTruckHealth and bParrotVOon then
    local tVo = {
      "Parrot-In-Mission-MinorContract-Pir03-07",
      "Parrot-In-Mission-MinorContract-Pir03-06",
      "Parrot-In-Mission-MinorContract-Pir03-05",
      "Parrot-In-Mission-MinorContract-Pir03-14",
      "Parrot-In-Mission-MinorContract-Pir03-17",
      "Parrot-In-Mission-MinorContract-Pir03-07",
      "Parrot-In-Mission-MinorContract-Pir03-09",
      "Parrot-In-Mission-MinorContract-Pir03-09",
      "Parrot-In-Mission-MinorContract-Pir03-07",
      "Parrot-In-Mission-MinorContract-Pir03-06"
    }
    local sSelectedVo = MrxUtil.GetRandomTableElement(tVo)
    local sSelectedVo2 = MrxUtil.GetRandomTableElement(tVo)
    VO.CueWithoutSubtitles(0, sSelectedVo)
    self:_CreateEvent(Event.TimerRelative, {1.5}, VO.CueWithoutSubtitles, {0, sSelectedVo})
    self:_CreateEvent(Event.TimerRelative, {15}, DamageVO, {self})
  else
    self:_CreateEvent(Event.TimerRelative, {5}, DamageVO, {self})
  end
end

function ParrotLostVO(self)
  if ParrotChat == 0 and bParrotVOon then
    local tVo = {
      "Parrot-In-Mission-MinorContract-Pir03-16",
      "Parrot-In-Mission-MinorContract-Pir03-16",
      "Parrot-In-Mission-MinorContract-Pir03-19",
      "Parrot-In-Mission-MinorContract-Pir03-15",
      "Parrot-In-Mission-MinorContract-Pir03-07",
      "Parrot-In-Mission-MinorContract-Pir03-07",
      "Parrot-In-Mission-MinorContract-Pir03-07",
      "Parrot-In-Mission-MinorContract-Pir03-07",
      "Parrot-In-Mission-MinorContract-Pir03-07",
      "Parrot-In-Mission-MinorContract-Pir03-08",
      "Parrot-In-Mission-MinorContract-Pir03-08",
      "Parrot-In-Mission-MinorContract-Pir03-06",
      "Parrot-In-Mission-MinorContract-Pir03-07",
      "Parrot-In-Mission-MinorContract-Pir03-06",
      "Parrot-In-Mission-MinorContract-Pir03-05",
      "Parrot-In-Mission-MinorContract-Pir03-14",
      "Parrot-In-Mission-MinorContract-Pir03-14",
      "Parrot-In-Mission-MinorContract-Pir03-17",
      "Parrot-In-Mission-MinorContract-Pir03-07",
      "Parrot-In-Mission-MinorContract-Pir03-09"
    }
    local sSelectedVo = MrxUtil.GetRandomTableElement(tVo)
    local sSelectedVo2 = MrxUtil.GetRandomTableElement(tVo)
    VO.CueWithoutSubtitles(0, sSelectedVo)
    self:_CreateEvent(Event.TimerRelative, {1}, VO.CueWithoutSubtitles, {0, sSelectedVo})
    ParrotChat = 1
    self:_CreateEvent(Event.TimerRelative, {7}, ParrotCooldown, {self})
  end
end

function ParrotCooldown(self)
  ParrotChat = 0
end

function SpeedVO(self)
  uTruck = Pg.GetGuidByName("ParrotPickup")
  if Object.IsAwake(uTruck) then
    nTruckSpeed = Object.GetVelocity(uTruck)
    if nTruckSpeed > 25 and bParrotVOon then
      Debug.Printf("Speeder!11   Going fast!")
      local tVo = {
        "Parrot-In-Mission-MinorContract-Pir03-07",
        "Parrot-In-Mission-MinorContract-Pir03-06",
        "Parrot-In-Mission-MinorContract-Pir03-11",
        "Parrot-In-Mission-MinorContract-Pir03-07",
        "Parrot-In-Mission-MinorContract-Pir03-06",
        "Parrot-In-Mission-MinorContract-Pir03-05",
        "Parrot-In-Mission-MinorContract-Pir03-05",
        "Parrot-In-Mission-MinorContract-Pir03-14",
        "Parrot-In-Mission-MinorContract-Pir03-07",
        "Parrot-In-Mission-MinorContract-Pir03-09",
        "Parrot-In-Mission-MinorContract-Pir03-15",
        "Parrot-In-Mission-MinorContract-Pir03-07",
        "Parrot-In-Mission-MinorContract-Pir03-07",
        "Parrot-In-Mission-MinorContract-Pir03-07",
        "Parrot-In-Mission-MinorContract-Pir03-07",
        "Parrot-In-Mission-MinorContract-Pir03-07"
      }
      local sSelectedVo = MrxUtil.GetRandomTableElement(tVo)
      VO.CueWithoutSubtitles(0, sSelectedVo)
      self:_CreateEvent(Event.TimerRelative, {20}, SpeedVO, {self})
    else
      self:_CreateEvent(Event.TimerRelative, {5}, SpeedVO, {self})
    end
  end
end

function CliffyA(self)
  uBlockParam = {
    AIGuid = Vehicle.GetDriver(Pg.GetGuidByName("CliffyA")),
    Goal = "PathMove",
    Target = Pg.GetGuidByName("Pa_CliffyA"),
    Priority = "hiPri"
  }
  self:_CreateEvent(Event.TimerRelative, {0.3}, Ai.Goal, {uBlockParam})
  uBlockParam2 = {
    AIGuid = Vehicle.GetDriver(Pg.GetGuidByName("CliffyB")),
    Goal = "PathMove",
    Target = Pg.GetGuidByName("Pa_CliffyB"),
    Priority = "hiPri"
  }
  self:_CreateEvent(Event.TimerRelative, {1}, Ai.Goal, {uBlockParam2})
end

function CheckGoodsLost(self)
  local nGoodCopy = nGoods
  local uMainTruck = Pg.GetGuidByName("ParrotPickup")
  if uMainTruck then
    if Object.IsAlive(uMainTruck) then
      if not MrxUtil.TestDistanceToAllPlayers(uMainTruck, 35, false, true) then
        self:_CreateEvent(Event.TimerRelative, {1}, function(self)
          if not MrxUtil.TestDistanceToAllPlayers(uMainTruck, 35, false, true) then
            local x, y, z = Object.GetHardpointPosition(uMainTruck, "HP_Truckbed")
            if x and y and z then
              local tGoods = Pg.GetObjectsInArea(x, y, z, 1, "BirdBox")
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
    local uBonusTruck = Pg.GetGuidByName("ParrotPickup_2")
    if uBonusTruck then
      if Object.IsAlive(uBonusTruck) then
        if not MrxUtil.TestDistanceToAllPlayers(uBonusTruck, 45, false, true) then
          self:_CreateEvent(Event.TimerRelative, {1}, function(self)
            if not MrxUtil.TestDistanceToAllPlayers(uMainTruck, 35, false, true) then
              local x, y, z = Object.GetHardpointPosition(uBonusTruck, "HP_Truckbed")
              if x and y and z then
                local tGoods2 = Pg.GetObjectsInArea(x, y, z, 1, "BirdBox")
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
    ParrotLostVO(self)
    DisplayGoodsLost(self)
  end
end

function DisplayGoodsLost(self)
  nCash = nGoods * nCargoValue
  sHudText = "[0x85070644]: " .. MrxUtil.FormatMoney(nCash)
  Hud.ObjectiveTray:SetSlotToText({
    vPlayer = nil,
    nSlot = 1,
    sText = sHudText
  })
  
  function _TooManyBirdsLost(self, nRequiredCash, sVoLine)
    gbLostTooManyBirds = true
    bParrotVOon = false
    MrxMusic.StopSpecialMusic("none")
    self:_SetCancelMessage("[PirCon003.Terms.Cancel01]")
    MrxVoSequence.Start({
      sVoLine,
      {
        self.Cancel,
        {self}
      }
    })
  end
  
  if nGoods <= 37 and nPlayed1 == 0 then
    nPlayed1 = 1
  end
  if nGoods < nRequired and not gbLostTooManyBirds then
    self:_TooManyBirdsLost(nRequiredCash, "Fiona-In-Mission-MinorContract-Pir03-27")
  end
end

function ActivateDelivered(self)
  self:CreateChild({
    sName = "PirCon003 MP End",
    sModuleName = "MrxTaskObjectiveAction",
    vTgtInclude = "PirCon003_endTalk",
    sDspShortDesc = "[PirCon003.Objectives.end]",
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
  bParrotVOon = false
  local x, y, z = Object.GetPosition(Pg.GetGuidByName("loc_DeliverySpot_end"))
  local tGoodsDelivered = {}
  local tGoodsDelivered = Pg.GetObjectsInArea(x, y, z, 25, "BirdBox")
  local nGoodsDelivered = table.getn(tGoodsDelivered)
  local iCashReward = nGoodsDelivered * nCargoValue
  if nGoodsDelivered == 42 and not bClientwasIn then
    self:_SetPlayer1Bonus(2000000 + iCashReward)
  elseif nGoodsDelivered == 84 and bClientwasIn then
    local MPBonus = 2000000 + iCashReward / 2
    self:_SetPlayer1Bonus(MPBonus)
    self:_SetPlayer2Bonus(MPBonus)
  elseif Net.IsActive() then
    self:_SetPlayer1Bonus(iCashReward / 2)
    self:_SetPlayer2Bonus(iCashReward / 2)
  else
    self:_SetPlayer1Bonus(iCashReward)
  end
  if 1 <= nGoodsDelivered then
    MrxVoSequence.Start({
      "PirThug-In-Mission-MinorContract-Pir02-06",
      "Fiona-In-Mission-MinorContract-Pir02-05",
      {
        self.Complete,
        {self}
      }
    })
  else
    MrxVoSequence.Start({
      "PirThug-In-Mission-MinorContract-Pir02-08"
    })
    self:_SetCancelMessage("[PirCon003.Terms.Cancel01]")
    self:_CreateEvent(Event.TimerRelative, {4}, self.Cancel, {self})
  end
end

function SetupMPGame(self)
  tLayersToAdd = {
    "vz_State_PirCon003_MP"
  }
  MrxLayerManager.Add(tLayersToAdd)
  tPickups = {
    "ParrotPickup",
    "ParrotPickup_2"
  }
  bClientwasIn = true
  nGoods2 = 42
  nGoodCopy = 84
  self:_CreateEvent(Event.ObjectDeath, {
    Pg.GetGuidByName("PirCon003_endTalk")
  }, function(self)
    self:_SetCancelMessage("[PirCon003.Terms.Cancel04]")
    self:Cancel()
  end, {self})
  self:_CreateEvent(Event.ObjectHibernation, {
    Pg.GetGuidByName("PirCon003_endTalk"),
    "awake"
  }, function(self)
    Ai.Goal({
      AIGuid = Pg.GetGuidByName("PirCon003_endTalk"),
      Goal = "Idle",
      Priority = "hiPri"
    })
  end, {self})
end

function Cleanup(self)
  Hud.ObjectiveTray:ClearSlot({vPlayer = nil, nSlot = 1})
  Hud.ObjectiveTray:ClearSlot({vPlayer = nil, nSlot = 2})
  MrxLayerManager.Remove("vz_state_PirCon003_Deliverables")
  if bClientwasIn then
    MrxLayerManager.Remove("vz_state_PirCon003_MP")
  end
  MrxFactionManager.ClearCustomPursuit()
  MrxMusic.StopSpecialMusic("none")
  MrxTaskContract.Cleanup(self)
end

function DaCustomPursuit(self, nLevel)
  if nLevel == 0 then
    tPursuitTable = {
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
          {"Car", 3}
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
      },
      {
        "Heli",
        {
          {
            "Car",
            "M113 (VZ) (DriverGunner)",
            1
          }
        },
        {
          {"Car", 2}
        }
      }
    }
  elseif nLevel == 1 then
    tPursuitTable = {
      {
        "Driving",
        {
          {
            "Car",
            "M151 .50Cal (VZ) (DriverGunner)",
            1
          },
          {
            "Car",
            "M35 (Guntruck) (VZ) (Full)",
            1
          }
        },
        {
          {"Car", 5}
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
          {"Car", 3}
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
      },
      {
        "Heli",
        {
          {
            "Heli",
            "Alouette3 Superiority (Driver)",
            1
          }
        },
        {
          {"Heli", 1}
        }
      }
    }
  elseif nLevel == 2 then
    tPursuitTable = {
      {
        "Driving",
        {
          {
            "Car",
            "M151 .50Cal (VZ) (DriverGunner)",
            1
          },
          {
            "Car",
            "M35 (Guntruck) (VZ) (Full)",
            1
          },
          {
            "Heli",
            "Alouette3 Attack (VZ) (Driver)",
            1
          }
        },
        {
          {"Car", 4},
          {"Heli", 1}
        }
      },
      {
        "Stopped",
        {
          {
            "Car",
            "M151 .50Cal (VZ) (DriverGunner)",
            1
          },
          {
            "Heli",
            "Alouette3 Attack (VZ) (Driver)",
            1
          }
        },
        {
          {"Car", 3},
          {"Heli", 1}
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
      },
      {
        "Heli",
        {
          {
            "Heli",
            "Alouette3 Superiority (Driver)",
            1
          }
        },
        {
          {"Heli", 2}
        }
      }
    }
  end
  MrxFactionManager.SetCustomPursuit(Pg.GetGuidByName("VZ"), -1, tPursuitTable)
end
