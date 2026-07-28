import("MrxGui")
import("MrxUtil")
import("MrxPmc")
import("mrxsupport")
import("mrxartillery")
import("mrxboatdelivery")
import("mrxbombingrun")
import("mrxbunkerbuster")
import("mrxcarpetbomb")
import("mrxsatclusterbomb")
import("mrxclusterbomb")
import("mrxcombatairpatrol")
import("mrxcratedelivery")
import("mrxcruisemissile")
import("mrxdaisycutter")
import("mrxfuelairbomb")
import("mrxgunship")
import("mrxharmstrike")
import("mrxlaserguidedbomb")
import("mrxmoab")
import("mrxrocketartillery")
import("mrxsatelliteguidedbomb")
import("mrxsmartbomb")
import("MrxSoldierDelivery")
import("mrxstrategicmissile")
import("mrxsupportpickup")
import("mrxsupporttransit")
import("mrxsurgicalstrike")
import("mrxtankbuster")
import("MrxOilCon002Delivery")
import("MrxMunitionsPickup")
import("MrxSupportPickup")
import("mrxsupportcopterdelivery")
import("MrxAchievements")
import("MrxFactionManager")
import("MrxShop")
import("WifMissionFlow")
_kMaxStock = 99
tSupportData = {}
tFreebieData = {}
tRequirementsObtained = {}
tRequirementStrings = {}
bIgnoreRequirements = false

function IsSupportEquippable(sKey)
  if bIgnoreRequirements then
    return true
  end
  if not sKey then
    return false
  end
  local tData = tSupportData[sKey]
  if not tData or not tData.oSupport then
    return false
  end
  local sRecruit = tData.oSupport:GetRecruit()
  if sRecruit and not tRequirementsObtained[sRecruit] then
    return false, tRequirementStrings[sRecruit]
  end
  if tData.tRequirementList then
    for n, sRequirement in ipairs(tData.tRequirementList) do
      if not tRequirementsObtained[sRequirement] then
        Debug.Printf("- Optional Requirement \"" .. tostring(sRequirement) .. "\" for support \"" .. tostring(sKey) .. "\" not obtained.")
        return false, tRequirementStrings[sRequirement]
      end
    end
  end
  return true
end

function SetHeliPilotRecruited(bRecruited)
  tRequirementsObtained.Copter = bRecruited
  if Net.IsServer() then
    Net.SendEvent_RecruitsUnlocked(tRequirementsObtained)
  end
end

function SetMechanicRecruited(bRecruited)
  tRequirementsObtained.Mechanic = bRecruited
  if Net.IsServer() then
    Net.SendEvent_RecruitsUnlocked(tRequirementsObtained)
  end
end

function SetJetPilotRecruited(bRecruited)
  tRequirementsObtained.Pilot = bRecruited
  if Net.IsServer() then
    Net.SendEvent_RecruitsUnlocked(tRequirementsObtained)
  end
end

function SetRequirement(sRequirement, bObtained)
  tRequirementsObtained[sRequirement] = bObtained
  if Net.IsServer() then
    Net.SendEvent_RecruitsUnlocked(tRequirementsObtained)
  end
end

function SynchNetRecruits(tRecruits)
  if Net.IsServer() then
    Net.SendEvent_RecruitsUnlocked(tRequirementsObtained)
  elseif tRecruits then
    SetRequirement("Fiona", tRecruits.Fiona)
    SetHeliPilotRecruited(tRecruits.Copter)
    SetMechanicRecruited(tRecruits.Mechanic)
    SetJetPilotRecruited(tRecruits.Pilot)
  end
end

function SetIgnoreRequirements(bIgnore)
  bIgnoreRequirements = bIgnore
end

function Init()
  tRequirementsObtained = {
    Fiona = true,
    Copter = true,
    Mechanic = true,
    Pilot = true
  }
  tRequirementStrings = {
    Fiona = "[PDA.Support.EquipFail.Fiona]",
    Copter = "[PDA.Support.EquipFail.Copter]",
    Mechanic = "[PDA.Support.EquipFail.Mechanic]",
    Pilot = "[PDA.Support.EquipFail.Pilot]"
  }
  local oSupport
  oSupport = mrxcratedelivery:Create()
  oSupport:SetCargo("Supply Drop (AA)")
  oSupport:SetCareless(true)
  tSupportData.aa = {
    sName = "[support.supply.aa.name]",
    sDescription = "[support.supply.aa.desc]",
    sIcon = "supplies_anti_air",
    nMaxStock = 99,
    nCashCost = 15000,
    nFuelCost = 40,
    oSupport = oSupport,
    sType = "Supply"
  }
  oSupport = mrxsupportcopterdelivery:Create()
  oSupport:SetDeliveryVehicle("AH1Z (Ewan)")
  tSupportData.ah1z = {
    sName = "[vehicle.ah1z]",
    sDescription = "[support.vehicle.ah1z.desc]",
    sIcon = "vehicles_heli_ah1z",
    nMaxStock = 99,
    nCashCost = 200000,
    nFuelCost = 180,
    oSupport = oSupport,
    sType = "Heli"
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetCargo("Supply Drop (Allied)")
  oSupport:SetCareless(true)
  tSupportData.al = {
    sName = "[support.supply.al.name]",
    sDescription = "[support.supply.al.desc]",
    sIcon = "supplies_AN_crate",
    nMaxStock = 99,
    nCashCost = 10000,
    nFuelCost = 40,
    oSupport = oSupport,
    sType = "Supply"
  }
  oSupport = mrxsupportcopterdelivery:Create()
  oSupport:SetDeliveryVehicle("Alouette3 Attack (PR) (Ewan)")
  tSupportData.alouette3attackpr = {
    sName = "[vehicle.alouette3attackpr]",
    sDescription = "[support.vehicle.alouette3attackpr.desc]",
    sIcon = "vehicles_heli_alouette",
    nMaxStock = 99,
    nCashCost = 75000,
    nFuelCost = 140,
    oSupport = oSupport,
    sType = "Heli"
  }
  oSupport = mrxsupportcopterdelivery:Create()
  oSupport:SetDeliveryVehicle("Alouette3 Attack (VZ) (Ewan)")
  tSupportData.alouette3attackvz = {
    sName = "[vehicle.alouette3attackvz]",
    sDescription = "[support.vehicle.alouette3attackvz.desc]",
    sIcon = "vehicles_heli_alouette",
    nMaxStock = 99,
    nCashCost = 75000,
    nFuelCost = 140,
    oSupport = oSupport,
    sType = "Heli"
  }
  oSupport = mrxsupportcopterdelivery:Create()
  oSupport:SetDeliveryVehicle("Alouette3 Elite (Ewan)")
  tSupportData.alouette3elite = {
    sName = "[vehicle.alouette3elite]",
    sDescription = "[support.vehicle.alouette3elite.desc]",
    sIcon = "vehicles_heli_alouette",
    nMaxStock = 99,
    nCashCost = 100000,
    nFuelCost = 140,
    oSupport = oSupport,
    sType = "Heli"
  }
  oSupport = mrxsupportcopterdelivery:Create()
  oSupport:SetDeliveryVehicle("Alouette3 Superiority (Ewan)")
  tSupportData.alouette3superiority = {
    sName = "[vehicle.alouette3superiority]",
    sDescription = "[support.vehicle.alouette3superiority.desc]",
    sIcon = "vehicles_heli_alouette",
    nMaxStock = 99,
    nCashCost = 125000,
    nFuelCost = 140,
    oSupport = oSupport,
    sType = "Heli"
  }
  oSupport = mrxsupportcopterdelivery:Create()
  oSupport:SetDeliveryVehicle("Alouette3 Transport (PR) (Ewan)")
  tSupportData.alouette3transportpr = {
    sName = "[vehicle.alouette3transportpr]",
    sDescription = "[support.vehicle.alouette3transportpr.desc]",
    sIcon = "vehicles_heli_alouette",
    nMaxStock = 99,
    nCashCost = 50000,
    nFuelCost = 100,
    oSupport = oSupport,
    sType = "Heli"
  }
  oSupport = mrxsupportcopterdelivery:Create()
  oSupport:SetDeliveryVehicle("Alouette3 Transport (VZ) (Ewan)")
  tSupportData.alouette3transportvz = {
    sName = "[vehicle.alouette3transportvz]",
    sDescription = "[support.vehicle.alouette3transportvz.desc]",
    sIcon = "vehicles_heli_alouette",
    nMaxStock = 99,
    nCashCost = 50000,
    nFuelCost = 100,
    oSupport = oSupport,
    sType = "Heli"
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetCargo("Supply Drop (AM AL)")
  oSupport:SetCareless(true)
  tSupportData.amal = {
    sName = "[support.supply.amal.name]",
    sDescription = "[support.supply.amal.desc]",
    sIcon = "supplies_anti_material",
    nMaxStock = 99,
    nCashCost = 15000,
    nFuelCost = 40,
    oSupport = oSupport,
    sType = "Supply"
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetCargo("Supply Drop (AM CH)")
  oSupport:SetCareless(true)
  tSupportData.amch = {
    sName = "[support.supply.amch.name]",
    sDescription = "[support.supply.amch.desc]",
    sIcon = "supplies_anti_material",
    nMaxStock = 99,
    nCashCost = 15000,
    nFuelCost = 40,
    oSupport = oSupport,
    sType = "Supply"
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetCargo("AMX30")
  oSupport:SetDeliveryVehicle("Mi26 (PMC) (Driver)")
  tSupportData.amx30 = {
    sName = "[vehicle.amx30]",
    sDescription = "[support.vehicle.amx30.desc]",
    sIcon = "vehicles_tank_amx30",
    nMaxStock = 99,
    nCashCost = 100000,
    nFuelCost = 160,
    oSupport = oSupport,
    sType = "Heavy"
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetCargo("AMX30 AA")
  oSupport:SetDeliveryVehicle("Mi26 (PMC) (Driver)")
  tSupportData.amx30aa = {
    sName = "[vehicle.amx30aa]",
    sDescription = "[support.vehicle.amx30aa.desc]",
    sIcon = "vehicles_tank_mosquitoAA",
    nMaxStock = 99,
    nCashCost = 125000,
    nFuelCost = 160,
    oSupport = oSupport,
    sType = "Heavy"
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetCargo("AMX30 Elite")
  oSupport:SetDeliveryVehicle("Mi26 (PMC) (Driver)")
  tSupportData.amx30elite = {
    sName = "[vehicle.amx30elite]",
    sDescription = "[support.vehicle.amx30elite.desc]",
    sIcon = "vehicles_tank_amx30",
    nMaxStock = 99,
    nCashCost = 150000,
    nFuelCost = 160,
    oSupport = oSupport,
    sType = "Heavy"
  }
  oSupport = mrxartillery:Create()
  tSupportData.artillery = {
    sName = "[support.airstrike.artillery.name]",
    sDescription = "[support.airstrike.artillery.desc]",
    sIcon = "support_artillery",
    nMaxStock = 99,
    nCashCost = 150000,
    nFuelCost = 140,
    oSupport = oSupport,
    sType = "Airstrike"
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetCargo("Supply Drop (AT AL)")
  oSupport:SetCareless(true)
  tSupportData.atal = {
    sName = "[support.supply.atal.name]",
    sDescription = "[support.supply.atal.desc]",
    sIcon = "supplies_AN_anti_tank",
    nMaxStock = 99,
    nCashCost = 15000,
    nFuelCost = 40,
    oSupport = oSupport,
    sType = "Supply"
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetCargo("Supply Drop (AT CH)")
  oSupport:SetCareless(true)
  tSupportData.atch = {
    sName = "[support.supply.atch.name]",
    sDescription = "[support.supply.atch.desc]",
    sIcon = "supplies_CH_anti_tank",
    nMaxStock = 99,
    nCashCost = 15000,
    nFuelCost = 40,
    oSupport = oSupport,
    sType = "Supply"
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetCargo({
    "Sportbike (Civ)",
    "Chopper",
    "Offroad Motorcycle (GR)",
    "Offroad Motorcycle (AL)",
    "Scooter"
  })
  tSupportData.bike = {
    sName = "[vehicle.bike]",
    sDescription = "[support.vehicle.bike.desc]",
    sIcon = "vehicles_motorcycle_street",
    nMaxStock = 99,
    nCashCost = 10000,
    nFuelCost = 60,
    oSupport = oSupport,
    sType = "Civilian"
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetCargo("Supply Drop (Blanco)")
  oSupport:SetCareless(true)
  tSupportData.blanco = {
    sName = "[support.supply.blanco.name]",
    sDescription = "[support.supply.blanco.desc]",
    sIcon = "supplies_Blanco_crate",
    nMaxStock = 99,
    nCashCost = 5000,
    nFuelCost = 40,
    oSupport = oSupport,
    sType = "Supply"
  }
  oSupport = mrxbombingrun:Create()
  tSupportData.bombingrun = {
    sName = "[support.airstrike.bombingrun.name]",
    sDescription = "[support.airstrike.bombingrun.desc]",
    sIcon = "support_bombing_run",
    nMaxStock = 99,
    nCashCost = 75000,
    nFuelCost = 140,
    oSupport = oSupport,
    sType = "Airstrike"
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetCargo("Buggy (Hellfire)")
  tSupportData.buggyhellfire = {
    sName = "[vehicle.buggyhellfire]",
    sDescription = "[support.vehicle.buggyhellfire.desc]",
    sIcon = "vehicles_light_buggy_rocket",
    nMaxStock = 99,
    nCashCost = 150000,
    nFuelCost = 60,
    oSupport = oSupport,
    sType = "Light"
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetCargo("Buggy (PR)")
  tSupportData.buggypr = {
    sName = "[vehicle.buggypr]",
    sDescription = "[support.vehicle.buggypr.desc]",
    sIcon = "vehicles_scorpion",
    nMaxStock = 99,
    nCashCost = 25000,
    nFuelCost = 60,
    oSupport = oSupport,
    sType = "Light"
  }
  oSupport = mrxbunkerbuster:Create()
  tSupportData.bunkerbuster = {
    sName = "[support.airstrike.bunkerbuster.name]",
    sDescription = "[support.airstrike.bunkerbuster.desc]",
    sIcon = "support_bunker_buster",
    nMaxStock = 99,
    nCashCost = 200000,
    nFuelCost = 300,
    oSupport = oSupport,
    sType = "Airstrike"
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetCargo("Supply Drop (C4)")
  oSupport:SetCareless(true)
  tSupportData.c4 = {
    sName = "[support.supply.c4.name]",
    sDescription = "[support.supply.c4.desc]",
    sIcon = "supplies_demolitions",
    nMaxStock = 99,
    nCashCost = 10000,
    nFuelCost = 40,
    oSupport = oSupport,
    sType = "Supply"
  }
  oSupport = mrxcarpetbomb:Create()
  tSupportData.carpetbomb = {
    sName = "[support.airstrike.carpetbomb.name]",
    sDescription = "[support.airstrike.carpetbomb.desc]",
    sIcon = "support_carpet_bomb",
    nMaxStock = 99,
    nCashCost = 250000,
    nFuelCost = 280,
    oSupport = oSupport,
    sType = "Airstrike"
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetCargo("Supply Drop (Chinese)")
  oSupport:SetCareless(true)
  tSupportData.ch = {
    sName = "[support.supply.ch.name]",
    sDescription = "[support.supply.ch.desc]",
    sIcon = "supplies_CH_crate",
    nMaxStock = 99,
    nCashCost = 10000,
    nFuelCost = 40,
    oSupport = oSupport,
    sType = "Supply"
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetCargo({
    "Valiant",
    "Valiant (4door)",
    "RTR (Civ)",
    "Phoenix",
    "CRX",
    "Pony (normal)",
    "Taxi (Tercel)",
    "R90",
    "Thunder",
    "Ridgeline",
    "El Grande",
    "ESV (lowrider)"
  })
  tSupportData.civilian = {
    sName = "[vehicle.civilian]",
    sDescription = "[support.vehicle.civilian.desc]",
    sIcon = "vehicles_car_crx",
    nMaxStock = 99,
    nCashCost = 15000,
    nFuelCost = 60,
    oSupport = oSupport,
    sType = "Civilian"
  }
  oSupport = mrxclusterbomb:Create()
  tSupportData.clusterbomb = {
    sName = "[support.airstrike.clusterbomb.name]",
    sDescription = "[support.airstrike.clusterbomb.desc]",
    sIcon = "support_cluster_bomb",
    nMaxStock = 99,
    nCashCost = 75000,
    nFuelCost = 220,
    oSupport = oSupport,
    sType = "Airstrike"
  }
  oSupport = mrxsupportcopterdelivery:Create()
  oSupport:SetDeliveryVehicle("Coanda Attack (Ewan)")
  tSupportData.coandaattack = {
    sName = "[vehicle.coandaattack]",
    sDescription = "[support.vehicle.coandaattack.desc]",
    sIcon = "vehicles_heli_md500",
    nMaxStock = 99,
    nCashCost = 75000,
    nFuelCost = 100,
    oSupport = oSupport,
    sType = "Heli"
  }
  oSupport = mrxsupportcopterdelivery:Create()
  oSupport:SetDeliveryVehicle("Coanda Gunship (Ewan)")
  tSupportData.coandagunship = {
    sName = "[vehicle.coandagunship]",
    sDescription = "[support.vehicle.coandagunship.desc]",
    sIcon = "vehicles_heli_md500",
    nMaxStock = 99,
    nCashCost = 100000,
    nFuelCost = 100,
    oSupport = oSupport,
    sType = "Heli"
  }
  oSupport = mrxsupportcopterdelivery:Create()
  oSupport:SetDeliveryVehicle("Coanda Superiority (Ewan)")
  tSupportData.coandasuperiority = {
    sName = "[vehicle.coandasuperiority]",
    sDescription = "[support.vehicle.coandasuperiority.desc]",
    sIcon = "vehicles_heli_md500",
    nMaxStock = 99,
    nCashCost = 75000,
    nFuelCost = 100,
    oSupport = oSupport,
    sType = "Heli"
  }
  oSupport = mrxsupportcopterdelivery:Create()
  oSupport:SetDeliveryVehicle("Coanda Transport (Ewan)")
  tSupportData.coandatransport = {
    sName = "[vehicle.coandatransport]",
    sDescription = "[support.vehicle.coandatransport.desc]",
    sIcon = "vehicles_heli_md500",
    nMaxStock = 99,
    nCashCost = 50000,
    nFuelCost = 60,
    oSupport = oSupport,
    sType = "Heli"
  }
  oSupport = mrxcombatairpatrol:Create()
  tSupportData.combatairpatrol = {
    sName = "[support.airstrike.combatairpatrol.name]",
    sDescription = "[support.airstrike.combatairpatrol.desc]",
    sIcon = "support_combat_air_patrol",
    nMaxStock = 99,
    nCashCost = 150000,
    nFuelCost = 280,
    oSupport = oSupport,
    sType = "Airstrike"
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetCargo("Supply Drop (Covert)")
  oSupport:SetCareless(true)
  tSupportData.covert = {
    sName = "[support.supply.covert.name]",
    sDescription = "[support.supply.covert.desc]",
    sIcon = "supplies_covert",
    nMaxStock = 99,
    nCashCost = 10000,
    nFuelCost = 40,
    oSupport = oSupport,
    sType = "Supply"
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetCargo("Supply Drop (CQB)")
  oSupport:SetCareless(true)
  tSupportData.cqb = {
    sName = "[support.supply.cqb.name]",
    sDescription = "[support.supply.cqb.desc]",
    sIcon = "supplies_cqb",
    nMaxStock = 99,
    nCashCost = 5000,
    nFuelCost = 40,
    oSupport = oSupport,
    sType = "Supply"
  }
  oSupport = mrxcruisemissile:Create()
  tSupportData.cruisemissile = {
    sName = "[support.airstrike.cruisemissile.name]",
    sDescription = "[support.airstrike.cruisemissile.desc]",
    sIcon = "support_cruise_missle",
    nMaxStock = 99,
    nCashCost = 400000,
    nFuelCost = 160,
    oSupport = oSupport,
    sType = "Airstrike"
  }
  oSupport = mrxdaisycutter:Create()
  tSupportData.daisycutter = {
    sName = "[support.airstrike.daisycutter.name]",
    sDescription = "[support.airstrike.daisycutter.desc]",
    sIcon = "support_daisy_cutter",
    nMaxStock = 99,
    nCashCost = 250000,
    nFuelCost = 300,
    oSupport = oSupport,
    sType = "Airstrike"
  }
  oSupport = mrxboatdelivery:Create()
  oSupport:SetCargo({
    "Dinghy",
    "Small Fishing Boat",
    "turbosquid (civ)",
    "dinghy"
  })
  oSupport:SetCareless(true)
  tSupportData.dinghy = {
    sName = "[vehicle.dinghy]",
    sDescription = "[support.vehicle.dinghy.desc]",
    sIcon = "vehicles_boat_turbosquid",
    nMaxStock = 99,
    nCashCost = 15000,
    nFuelCost = 60,
    oSupport = oSupport,
    sType = "Boat"
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetCargo("DSV Scout Vehicle")
  tSupportData.dsvscoutvehicle = {
    sName = "[vehicle.dsvscoutvehicle]",
    sDescription = "[support.vehicle.dsvscoutvehicle.desc]",
    sIcon = "vehicles_scorpion",
    nMaxStock = 99,
    nCashCost = 150000,
    nFuelCost = 60,
    oSupport = oSupport,
    sType = "Light"
  }
  oSupport = mrxsupportcopterdelivery:Create()
  oSupport:SetDeliveryVehicle("Endriago (Attack) (Ewan)")
  tSupportData.endriagoattack = {
    sName = "[vehicle.endriagoattack]",
    sDescription = "[support.vehicle.endriagoattack.desc]",
    sIcon = "vehicles_heli_uh1",
    nMaxStock = 99,
    nCashCost = 45000,
    nFuelCost = 140,
    oSupport = oSupport,
    sType = "Heli"
  }
  oSupport = mrxsupportcopterdelivery:Create()
  oSupport:SetDeliveryVehicle("Endriago (Elite) (Ewan)")
  tSupportData.endriagoelite = {
    sName = "[vehicle.endriagoelite]",
    sDescription = "[support.vehicle.endriagoelite.desc]",
    sIcon = "vehicles_heli_uh1",
    nMaxStock = 99,
    nCashCost = 60000,
    nFuelCost = 140,
    oSupport = oSupport,
    sType = "Heli"
  }
  oSupport = mrxsupportcopterdelivery:Create()
  oSupport:SetDeliveryVehicle("Endriago (Superiority) (Ewan)")
  tSupportData.endriagosuperiority = {
    sName = "[vehicle.endriagosuperiority]",
    sDescription = "[support.vehicle.endriagosuperiority.desc]",
    sIcon = "vehicles_heli_uh1",
    nMaxStock = 99,
    nCashCost = 75000,
    nFuelCost = 140,
    oSupport = oSupport,
    sType = "Heli"
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetCargo("EXT")
  tSupportData.ext = {
    sName = "[vehicle.ext]",
    sDescription = "[support.vehicle.ext.desc]",
    sIcon = "vehicles_truck_ext",
    nMaxStock = 99,
    nCashCost = 20000,
    nFuelCost = 60,
    oSupport = oSupport,
    sType = "Light"
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetCargo("EXT (GL)")
  tSupportData.extgl = {
    sName = "[vehicle.extgl]",
    sDescription = "[support.vehicle.extgl.desc]",
    sIcon = "vehicles_truck_ext",
    nMaxStock = 99,
    nCashCost = 20000,
    nFuelCost = 60,
    oSupport = oSupport,
    sType = "Light"
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetCargo("Supply Drop (Fiona)")
  oSupport:SetCareless(true)
  tSupportData.fiona = {
    sName = "[support.supply.fiona.name]",
    sDescription = "[support.supply.fiona.desc]",
    sIcon = "supplies_PMC_crate",
    nMaxStock = 99,
    nCashCost = 5000,
    nFuelCost = 40,
    oSupport = oSupport,
    sType = "Supply"
  }
  oSupport = mrxfuelairbomb:Create()
  tSupportData.fuelairbomb = {
    sName = "[support.airstrike.fuelairbomb.name]",
    sDescription = "[support.airstrike.fuelairbomb.desc]",
    sIcon = "support_fuel_air_bomb",
    nMaxStock = 99,
    nCashCost = 200000,
    nFuelCost = 900,
    oSupport = oSupport,
    sType = "Airstrike"
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetCargo("Supply Drop (GL)")
  oSupport:SetCareless(true)
  tSupportData.gl = {
    sName = "[support.supply.gl.name]",
    sDescription = "[support.supply.gl.desc]",
    sIcon = "supplies_grenade_launcher",
    nMaxStock = 99,
    nCashCost = 10000,
    nFuelCost = 40,
    oSupport = oSupport,
    sType = "Supply"
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetCargo("Supply Drop (Guerilla)")
  oSupport:SetCareless(true)
  tSupportData.gr = {
    sName = "[support.supply.gr.name]",
    sDescription = "[support.supply.gr.desc]",
    sIcon = "supplies_GR_crate",
    nMaxStock = 99,
    nCashCost = 5000,
    nFuelCost = 40,
    oSupport = oSupport,
    sType = "Supply"
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetCargo("Guntruck (OC)")
  tSupportData.guntruckoc = {
    sName = "[vehicle.guntruckoc]",
    sDescription = "[support.vehicle.guntruckoc.desc]",
    sIcon = "vehicles_truck_transport",
    nMaxStock = 99,
    nCashCost = 20000,
    nFuelCost = 100,
    oSupport = oSupport,
    sType = "Light"
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetCargo("HMMWV (Armored) (50Cal)")
  tSupportData.hmmwvarmored50cal = {
    sName = "[vehicle.hmmwvarmored50cal]",
    sDescription = "[support.vehicle.hmmwvarmored50cal.desc]",
    sIcon = "vehicles_truck_hmmwv",
    nMaxStock = 99,
    nCashCost = 20000,
    nFuelCost = 120,
    oSupport = oSupport,
    sType = "Light"
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetCargo("HMMWV (Armored) (GL)")
  tSupportData.hmmwvarmoredgl = {
    sName = "[vehicle.hmmwvarmoredgl]",
    sDescription = "[support.vehicle.hmmwvarmoredgl.desc]",
    sIcon = "vehicles_truck_hmmwv",
    nMaxStock = 99,
    nCashCost = 25000,
    nFuelCost = 120,
    oSupport = oSupport,
    sType = "Light"
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetCargo("HMMWV (Armored) (TOW)")
  tSupportData.hmmwvarmoredtow = {
    sName = "[vehicle.hmmwvarmoredtow]",
    sDescription = "[support.vehicle.hmmwvarmoredtow.desc]",
    sIcon = "vehicles_truck_hmmwv",
    nMaxStock = 99,
    nCashCost = 35000,
    nFuelCost = 120,
    oSupport = oSupport,
    sType = "Light"
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetCargo("HMMWV (Avenger)")
  tSupportData.hmmwvavenger = {
    sName = "[vehicle.hmmwvavenger]",
    sDescription = "[support.vehicle.hmmwvavenger.desc]",
    sIcon = "vehicles_truck_hmmwv",
    nMaxStock = 99,
    nCashCost = 45000,
    nFuelCost = 120,
    oSupport = oSupport,
    sType = "Light"
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetCargo("HMMWV (Softtop)")
  tSupportData.hmmwvsofttop = {
    sName = "[vehicle.hmmwvsofttop]",
    sDescription = "[support.vehicle.hmmwvsofttop.desc]",
    sIcon = "vehicles_truck_hmmwv",
    nMaxStock = 99,
    nCashCost = 15000,
    nFuelCost = 120,
    oSupport = oSupport,
    sType = "Civilian"
  }
  oSupport = mrxboatdelivery:Create()
  oSupport:SetCargo({
    "Jetski (Civ)",
    "Jetski (PR)"
  })
  oSupport:SetCareless(true)
  tSupportData.jetskiciv = {
    sName = "[vehicle.jetskiciv]",
    sDescription = "[support.vehicle.jetskiciv.desc]",
    sIcon = "vehicles_boat_jetski",
    nMaxStock = 99,
    nCashCost = 5000,
    nFuelCost = 40,
    oSupport = oSupport,
    sType = "Boat"
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetCargo({
    "Phoenix (crappy)",
    "RTR (crappy)",
    "Valiant (crappy)",
    "Pony (crappy)",
    "Van (crappy)"
  })
  tSupportData.junkers = {
    sName = "[vehicle.junkers]",
    sDescription = "[support.vehicle.junkers.desc]",
    sIcon = "vehicles_car_pony",
    nMaxStock = 99,
    nCashCost = 5000,
    nFuelCost = 60,
    oSupport = oSupport,
    sType = "Civilian"
  }
  oSupport = mrxsupportcopterdelivery:Create()
  oSupport:SetDeliveryVehicle("Ka29b (Ewan)")
  tSupportData.ka29b = {
    sName = "[vehicle.ka29b]",
    sDescription = "[support.vehicle.ka29b.desc]",
    sIcon = "vehicles_heli_ka28",
    nMaxStock = 99,
    nCashCost = 40000,
    nFuelCost = 160,
    oSupport = oSupport,
    sType = "Heli"
  }
  oSupport = mrxlaserguidedbomb:Create()
  tSupportData.laserguidedbomb = {
    sName = "[support.airstrike.laserguidedbomb.name]",
    sDescription = "[support.airstrike.laserguidedbomb.desc]",
    sIcon = "support_laser_guided_bomb",
    nMaxStock = 99,
    nCashCost = 200000,
    nFuelCost = 460,
    oSupport = oSupport,
    sType = "Airstrike"
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetCargo("LAVIII (25mm)")
  tSupportData.laviii25mm = {
    sName = "[vehicle.laviii25mm]",
    sDescription = "[support.vehicle.laviii25mm.desc]",
    sIcon = "vehicles_apc_lavii",
    nMaxStock = 99,
    nCashCost = 25000,
    nFuelCost = 140,
    oSupport = oSupport,
    sType = "Light"
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetCargo("LAVIII (Minigun)")
  tSupportData.laviii50cal = {
    sName = "[vehicle.laviii50cal]",
    sDescription = "[support.vehicle.laviii50cal.desc]",
    sIcon = "vehicles_apc_lavii",
    nMaxStock = 99,
    nCashCost = 20000,
    nFuelCost = 140,
    oSupport = oSupport,
    sType = "Light"
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetCargo("LAVIII (AD)")
  tSupportData.laviiiad = {
    sName = "[vehicle.laviiiad]",
    sDescription = "[support.vehicle.laviiiad.desc]",
    sIcon = "vehicles_apc_lavii",
    nMaxStock = 99,
    nCashCost = 30000,
    nFuelCost = 140,
    oSupport = oSupport,
    sType = "Light"
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetCargo("LAVIII (AT)")
  tSupportData.laviiiat = {
    sName = "[vehicle.laviiiat]",
    sDescription = "[support.vehicle.laviiiat.desc]",
    sIcon = "vehicles_apc_lavii",
    nMaxStock = 99,
    nCashCost = 30000,
    nFuelCost = 140,
    oSupport = oSupport,
    sType = "Light"
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetCargo("LAVIII (MEWSS)")
  tSupportData.laviiimewss = {
    sName = "[vehicle.laviiimewss]",
    sDescription = "[support.vehicle.laviiimewss.desc]",
    sIcon = "vehicles_apc_lavii",
    nMaxStock = 99,
    nCashCost = 30000,
    nFuelCost = 140,
    oSupport = oSupport,
    sType = "Light"
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetCargo("LAVIII (MGS)")
  tSupportData.laviiimgs = {
    sName = "[vehicle.laviiimgs]",
    sDescription = "[support.vehicle.laviiimgs.desc]",
    sIcon = "vehicles_apc_lavii",
    nMaxStock = 99,
    nCashCost = 40000,
    nFuelCost = 160,
    oSupport = oSupport,
    sType = "Light"
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetCargo("Supply Drop (Light MG)")
  oSupport:SetCareless(true)
  tSupportData.lightmg = {
    sName = "[support.supply.lightmg.name]",
    sDescription = "[support.supply.lightmg.desc]",
    sIcon = "supplies_light_mg",
    nMaxStock = 99,
    nCashCost = 5000,
    nFuelCost = 40,
    oSupport = oSupport,
    sType = "Supply"
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetCargo({
    "L300",
    "W8 (normal)",
    "W12 (normal)",
    "Vanquish",
    "ESV (Lowrider)"
  })
  tSupportData.luxury = {
    sName = "[vehicle.luxury]",
    sDescription = "[support.vehicle.luxury.desc]",
    sIcon = "vehicles_car_l300",
    nMaxStock = 99,
    nCashCost = 25000,
    nFuelCost = 60,
    oSupport = oSupport,
    sType = "Civilian"
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetCargo("M113 AA (GR)")
  oSupport:SetDeliveryVehicle("Mi26 (PMC) (Driver)")
  tSupportData.m113aagr = {
    sName = "[vehicle.m113aagr]",
    sDescription = "[support.vehicle.m113aagr.desc]",
    sIcon = "vehicles_apc_m113",
    nMaxStock = 99,
    nCashCost = 35000,
    nFuelCost = 100,
    oSupport = oSupport,
    sType = "Heavy"
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetCargo("M113 AA (VZ)")
  oSupport:SetDeliveryVehicle("Mi26 (PMC) (Driver)")
  tSupportData.m113aavz = {
    sName = "[vehicle.m113aavz]",
    sDescription = "[support.vehicle.m113aavz.desc]",
    sIcon = "vehicles_apc_m113",
    nMaxStock = 99,
    nCashCost = 40000,
    nFuelCost = 100,
    oSupport = oSupport,
    sType = "Heavy"
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetCargo("M113 (GR)")
  oSupport:SetDeliveryVehicle("Mi26 (PMC) (Driver)")
  tSupportData.m113gr = {
    sName = "[vehicle.m113gr]",
    sDescription = "[support.vehicle.m113gr.desc]",
    sIcon = "vehicles_apc_m113",
    nMaxStock = 99,
    nCashCost = 30000,
    nFuelCost = 100,
    oSupport = oSupport,
    sType = "Heavy"
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetCargo("M113 Jammer (VZ)")
  oSupport:SetDeliveryVehicle("Mi26 (PMC) (Driver)")
  tSupportData.m113jammervz = {
    sName = "[vehicle.m113jammervz]",
    sDescription = "[support.vehicle.m113jammervz.desc]",
    sIcon = "vehicles_apc_m113",
    nMaxStock = 99,
    nCashCost = 40000,
    nFuelCost = 100,
    oSupport = oSupport,
    sType = "Heavy"
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetCargo("M113 (VZ)")
  oSupport:SetDeliveryVehicle("Mi26 (PMC) (Driver)")
  tSupportData.m113vz = {
    sName = "[vehicle.m113vz]",
    sDescription = "[support.vehicle.m113vz.desc]",
    sIcon = "vehicles_apc_m113",
    nMaxStock = 99,
    nCashCost = 35000,
    nFuelCost = 100,
    oSupport = oSupport,
    sType = "Heavy"
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetCargo("M151 (MG) (GR)")
  tSupportData.m15150calgr = {
    sName = "[vehicle.m15150calgr]",
    sDescription = "[support.vehicle.m15150calgr.desc]",
    sIcon = "vehicles_truck_m151",
    nMaxStock = 99,
    nCashCost = 15000,
    nFuelCost = 60,
    oSupport = oSupport,
    sType = "Light"
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetCargo("M151 .50Cal (VZ)")
  tSupportData.m15150calvz = {
    sName = "[vehicle.m15150calvz]",
    sDescription = "[support.vehicle.m15150calvz.desc]",
    sIcon = "vehicles_truck_m151",
    nMaxStock = 99,
    nCashCost = 15000,
    nFuelCost = 60,
    oSupport = oSupport,
    sType = "Light"
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetCargo("M151 Softtop (GR)")
  tSupportData.m151softtopgr = {
    sName = "[vehicle.m151softtopgr]",
    sDescription = "[support.vehicle.m151softtopgr.desc]",
    sIcon = "vehicles_truck_m151",
    nMaxStock = 99,
    nCashCost = 10000,
    nFuelCost = 60,
    oSupport = oSupport,
    sType = "Civilian"
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetCargo("M151 Softtop (VZ)")
  tSupportData.m151softtopvz = {
    sName = "[vehicle.m151softtopvz]",
    sDescription = "[support.vehicle.m151softtopvz.desc]",
    sIcon = "vehicles_truck_m151",
    nMaxStock = 99,
    nCashCost = 10000,
    nFuelCost = 60,
    oSupport = oSupport,
    sType = "Civilian"
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetCargo("M1A2")
  oSupport:SetDeliveryVehicle("Mi26 (PMC) (Driver)")
  tSupportData.m1a2 = {
    sName = "[vehicle.m1a2]",
    sDescription = "[support.vehicle.m1a2.desc]",
    sIcon = "vehicles_tank_m1a2",
    nMaxStock = 99,
    nCashCost = 425000,
    nFuelCost = 240,
    oSupport = oSupport,
    sType = "Heavy"
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetCargo("M2A3")
  oSupport:SetDeliveryVehicle("Mi26 (PMC) (Driver)")
  tSupportData.m2a3 = {
    sName = "[vehicle.m2a3]",
    sDescription = "[support.vehicle.m2a3.desc]",
    sIcon = "vehicles_tank_m6",
    nMaxStock = 99,
    nCashCost = 150000,
    nFuelCost = 160,
    oSupport = oSupport,
    sType = "Heavy"
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetCargo("M35 (AA) (GR)")
  tSupportData.m35aagr = {
    sName = "[vehicle.m35aagr]",
    sDescription = "[support.vehicle.m35aagr.desc]",
    sIcon = "vehicles_truck_m35",
    nMaxStock = 99,
    nCashCost = 25000,
    nFuelCost = 80,
    oSupport = oSupport,
    sType = "Light"
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetCargo("M35 (AA) (VZ)")
  tSupportData.m35aavz = {
    sName = "[vehicle.m35aavz]",
    sDescription = "[support.vehicle.m35aavz.desc]",
    sIcon = "vehicles_truck_m35",
    nMaxStock = 99,
    nCashCost = 30000,
    nFuelCost = 80,
    oSupport = oSupport,
    sType = "Light"
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetCargo("M35 (Guntruck) (GR)")
  tSupportData.m35guntruckgr = {
    sName = "[vehicle.m35guntruckgr]",
    sDescription = "[support.vehicle.m35guntruckgr.desc]",
    sIcon = "vehicles_truck_m35",
    nMaxStock = 99,
    nCashCost = 15000,
    nFuelCost = 60,
    oSupport = oSupport,
    sType = "Light"
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetCargo("M35 (Guntruck) (VZ)")
  tSupportData.m35guntruckvz = {
    sName = "[vehicle.m35guntruckvz]",
    sDescription = "[support.vehicle.m35guntruckvz.desc]",
    sIcon = "vehicles_truck_m35",
    nMaxStock = 99,
    nCashCost = 20000,
    nFuelCost = 60,
    oSupport = oSupport,
    sType = "Light"
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetCargo("M551")
  oSupport:SetDeliveryVehicle("Mi26 (PMC) (Driver)")
  tSupportData.m551 = {
    sName = "[vehicle.m551]",
    sDescription = "[support.vehicle.m551.desc]",
    sIcon = "vehicles_tank_m551",
    nMaxStock = 99,
    nCashCost = 45000,
    nFuelCost = 140,
    oSupport = oSupport,
    sType = "Heavy"
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetCargo("Mattias Chopper")
  tSupportData.mattiaschopper = {
    sName = "[vehicle.mattiaschopper]",
    sDescription = "[support.vehicle.mattiaschopper.desc]",
    sIcon = "vehicles_motorcycle_chopper",
    nMaxStock = 99,
    nCashCost = 25000,
    nFuelCost = 40,
    oSupport = oSupport,
    sType = "Civilian"
  }
  oSupport = mrxsupportcopterdelivery:Create()
  oSupport:SetDeliveryVehicle("MH53J (Ewan)")
  tSupportData.mh53j = {
    sName = "[vehicle.mh53j]",
    sDescription = "[support.vehicle.mh53j.desc]",
    sIcon = "vehicles_heli_mi26",
    nMaxStock = 99,
    nCashCost = 50000,
    nFuelCost = 140,
    oSupport = oSupport,
    sType = "Heli"
  }
  oSupport = mrxsupportcopterdelivery:Create()
  oSupport:SetDeliveryVehicle("Mi26 (CH) (Ewan)")
  tSupportData.mi26ch = {
    sName = "[vehicle.mi26ch]",
    sDescription = "[support.vehicle.mi26ch.desc]",
    sIcon = "vehicles_heli_mi26",
    nMaxStock = 99,
    nCashCost = 50000,
    nFuelCost = 140,
    oSupport = oSupport,
    sType = "Heli"
  }
  oSupport = mrxsupportcopterdelivery:Create()
  oSupport:SetDeliveryVehicle("Mi26 (VZ) (Ewan)")
  tSupportData.mi26vz = {
    sName = "[vehicle.mi26vz]",
    sDescription = "[support.vehicle.mi26vz.desc]",
    sIcon = "vehicles_heli_mi26",
    nMaxStock = 99,
    nCashCost = 50000,
    nFuelCost = 140,
    oSupport = oSupport,
    sType = "Heli"
  }
  oSupport = mrxsupportcopterdelivery:Create()
  oSupport:SetDeliveryVehicle("Mi35 (Ewan)")
  tSupportData.mi35 = {
    sName = "[vehicle.mi35]",
    sDescription = "[support.vehicle.mi35.desc]",
    sIcon = "vehicles_heli_mi35",
    nMaxStock = 99,
    nCashCost = 250000,
    nFuelCost = 200,
    oSupport = oSupport,
    sType = "Heli"
  }
  oSupport = mrxmoab:Create()
  tSupportData.moab = {
    sName = "[support.airstrike.moab.name]",
    sDescription = "[support.airstrike.moab.desc]",
    sIcon = "support_moab",
    nMaxStock = 99,
    nCashCost = 500000,
    nFuelCost = 400,
    oSupport = oSupport,
    sType = "Airstrike"
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetCargo({
    "Monster Ridgeline",
    "EXT (Monster)",
    "Cougar"
  })
  tSupportData.monster = {
    sName = "[vehicle.monster]",
    sDescription = "[support.vehicle.monster.desc]",
    sIcon = "vehicles_car_monsterTruck",
    nMaxStock = 99,
    nCashCost = 20000,
    nFuelCost = 60,
    oSupport = oSupport,
    sType = "Civilian"
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetCargo("Monster Truck")
  tSupportData.monstertruck = {
    sName = "[vehicle.monstertruck]",
    sDescription = "[support.vehicle.monstertruck.desc]",
    sIcon = "vehicles_car_monsterCar",
    nMaxStock = 99,
    nCashCost = 40000,
    nFuelCost = 60,
    oSupport = oSupport,
    sType = "Civilian"
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetCargo("NGLV (MG)")
  tSupportData.nglv50cal = {
    sName = "[vehicle.nglv50cal]",
    sDescription = "[support.vehicle.nglv50cal.desc]",
    sIcon = "vehicles_truck_nglv",
    nMaxStock = 99,
    nCashCost = 35000,
    nFuelCost = 80,
    oSupport = oSupport,
    sType = "Light"
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetCargo("NGLV (GL)")
  tSupportData.nglvgl = {
    sName = "[vehicle.nglvgl]",
    sDescription = "[support.vehicle.nglvgl.desc]",
    sIcon = "vehicles_truck_nglv",
    nMaxStock = 99,
    nCashCost = 45000,
    nFuelCost = 80,
    oSupport = oSupport,
    sType = "Light"
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetCargo("Supply Drop (OC)")
  oSupport:SetCareless(true)
  tSupportData.oc = {
    sName = "[support.supply.oc.name]",
    sDescription = "[support.supply.oc.desc]",
    sIcon = "supplies_OC_crate",
    nMaxStock = 99,
    nCashCost = 5000,
    nFuelCost = 40,
    oSupport = oSupport,
    sType = "Supply"
  }
  oSupport = mrxboatdelivery:Create()
  oSupport:SetCargo("Omen")
  oSupport:SetCareless(true)
  tSupportData.omen = {
    sName = "[vehicle.omen]",
    sDescription = "[support.vehicle.omen.desc]",
    sIcon = "vehicles_boat_omen",
    nMaxStock = 99,
    nCashCost = 25000,
    nFuelCost = 60,
    oSupport = oSupport,
    sType = "Boat"
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetCargo("Panhard (Assault)")
  tSupportData.panhardassault = {
    sName = "[vehicle.panhardassault]",
    sDescription = "[support.vehicle.panhardassault.desc]",
    sIcon = "vehicles_light_vulcan",
    nMaxStock = 99,
    nCashCost = 400000,
    nFuelCost = 60,
    oSupport = oSupport,
    sType = "Light"
  }
  oSupport = mrxboatdelivery:Create()
  oSupport:SetCargo("Patrol Boat (PMC)")
  oSupport:SetCareless(true)
  tSupportData.patrolboatpmc = {
    sName = "[vehicle.patrolboatpmc]",
    sDescription = "[support.vehicle.patrolboatpmc.desc]",
    sIcon = "vehicles_boat_cigarette",
    nMaxStock = 99,
    nCashCost = 500000,
    nFuelCost = 60,
    oSupport = oSupport,
    sType = "Boat"
  }
  oSupport = mrxboatdelivery:Create()
  oSupport:SetCargo("Patrol Boat (VZ)")
  oSupport:SetCareless(true)
  tSupportData.patrolboatvz = {
    sName = "[vehicle.patrolboatvz]",
    sDescription = "[support.vehicle.patrolboatvz.desc]",
    sIcon = "vehicles_boat_Piranha",
    nMaxStock = 99,
    nCashCost = 75000,
    nFuelCost = 80,
    oSupport = oSupport,
    sType = "Boat"
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetCargo("PGZ95")
  oSupport:SetDeliveryVehicle("Mi26 (PMC) (Driver)")
  tSupportData.pgz95 = {
    sName = "[vehicle.pgz95]",
    sDescription = "[support.vehicle.pgz95.desc]",
    sIcon = "vehicles_tank_pgz95",
    nMaxStock = 99,
    nCashCost = 150000,
    nFuelCost = 160,
    oSupport = oSupport,
    sType = "Heavy"
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetCargo("PGZ95 Command")
  oSupport:SetDeliveryVehicle("Mi26 (PMC) (Driver)")
  tSupportData.pgz95command = {
    sName = "[vehicle.pgz95command]",
    sDescription = "[support.vehicle.pgz95command.desc]",
    sIcon = "vehicles_tank_pgz95",
    nMaxStock = 99,
    nCashCost = 165000,
    nFuelCost = 160,
    oSupport = oSupport,
    sType = "Heavy"
  }
  oSupport = mrxboatdelivery:Create()
  oSupport:SetCargo("Piranha")
  oSupport:SetCareless(true)
  tSupportData.piranha = {
    sName = "[vehicle.piranha]",
    sDescription = "[support.vehicle.piranha.desc]",
    sIcon = "vehicles_boat_prestes",
    nMaxStock = 99,
    nCashCost = 45000,
    nFuelCost = 60,
    oSupport = oSupport,
    sType = "Boat"
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetCargo("PLZ45")
  oSupport:SetDeliveryVehicle("Mi26 (PMC) (Driver)")
  tSupportData.plz45 = {
    sName = "[vehicle.plz45]",
    sDescription = "[support.vehicle.plz45.desc]",
    sIcon = "vehicles_tank_plz45",
    nMaxStock = 99,
    nCashCost = 100000,
    nFuelCost = 160,
    oSupport = oSupport,
    sType = "Heavy"
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetCargo("Supply Drop (Pirate)")
  oSupport:SetCareless(true)
  tSupportData.pr = {
    sName = "[support.supply.pr.name]",
    sDescription = "[support.supply.pr.desc]",
    sIcon = "supplies_cqb",
    nMaxStock = 99,
    nCashCost = 5000,
    nFuelCost = 40,
    oSupport = oSupport,
    sType = "Supply"
  }
  oSupport = mrxrocketartillery:Create()
  tSupportData.rocketartillery = {
    sName = "[support.airstrike.rocketartillery.name]",
    sDescription = "[support.airstrike.rocketartillery.desc]",
    sIcon = "support_rocket_artillery",
    nMaxStock = 99,
    nCashCost = 350000,
    nFuelCost = 280,
    oSupport = oSupport,
    sType = "Airstrike"
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetCargo("Supply Drop (RPG)")
  oSupport:SetCareless(true)
  tSupportData.rpg = {
    sName = "[support.supply.rpg.name]",
    sDescription = "[support.supply.rpg.desc]",
    sIcon = "supplies_rpg",
    nMaxStock = 99,
    nCashCost = 10000,
    nFuelCost = 40,
    oSupport = oSupport,
    sType = "Supply"
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetCargo("Scorpion90")
  oSupport:SetDeliveryVehicle("Mi26 (PMC) (Driver)")
  tSupportData.scorpion90 = {
    sName = "[vehicle.scorpion90]",
    sDescription = "[support.vehicle.scorpion90.desc]",
    sIcon = "vehicles_tank_scorpion",
    nMaxStock = 99,
    nCashCost = 75000,
    nFuelCost = 140,
    oSupport = oSupport,
    sType = "Heavy"
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetCargo("Sidecar Motorcycle")
  tSupportData.sidecarmotorcycle = {
    sName = "[vehicle.sidecarmotorcycle]",
    sDescription = "[support.vehicle.sidecarmotorcycle.desc]",
    sIcon = "vehicles_motorcycle_sidecar",
    nMaxStock = 99,
    nCashCost = 50000,
    nFuelCost = 40,
    oSupport = oSupport,
    sType = "Light"
  }
  oSupport = mrxsmartbomb:Create()
  tSupportData.smartbomb = {
    sName = "[support.airstrike.smartbomb.name]",
    sDescription = "[support.airstrike.smartbomb.desc]",
    sIcon = "support_smart_bomb",
    nMaxStock = 99,
    nCashCost = 300000,
    nFuelCost = 200,
    oSupport = oSupport,
    sType = "Airstrike"
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetCargo("Supply Drop (Sniper CH)")
  oSupport:SetCareless(true)
  tSupportData.sniperch = {
    sName = "[support.supply.sniperch.name]",
    sDescription = "[support.supply.sniperch.desc]",
    sIcon = "supplies_CH_sniper",
    nMaxStock = 99,
    nCashCost = 10000,
    nFuelCost = 40,
    oSupport = oSupport,
    sType = "Supply"
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetCargo("Supply Drop (Sniper RU)")
  oSupport:SetCareless(true)
  tSupportData.sniperru = {
    sName = "[support.supply.sniperru.name]",
    sDescription = "[support.supply.sniperru.desc]",
    sIcon = "supplies_sniper_kit",
    nMaxStock = 99,
    nCashCost = 10000,
    nFuelCost = 40,
    oSupport = oSupport,
    sType = "Supply"
  }
  oSupport = mrxboatdelivery:Create()
  oSupport:SetCargo("Speed Boat")
  oSupport:SetCareless(true)
  tSupportData.speedboat = {
    sName = "[vehicle.speedboat]",
    sDescription = "[support.vehicle.speedboat.desc]",
    sIcon = "vehicles_boat_cigarette",
    nMaxStock = 99,
    nCashCost = 10000,
    nFuelCost = 60,
    oSupport = oSupport,
    sType = "Boat"
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetCargo({
    "Vanquish (racing)",
    "Veyron",
    "W12 (sprint)",
    "W12 (Z12) Racer",
    "W12 (Z12)",
    "Phoenix (racing)",
    "RTR (racing)",
    "L300 (Racing)",
    "CRX (racing)"
  })
  tSupportData.sports = {
    sName = "[vehicle.sports]",
    sDescription = "[support.vehicle.sports.desc]",
    sIcon = "vehicles_car_phoenix",
    nMaxStock = 99,
    nCashCost = 20000,
    nFuelCost = 60,
    oSupport = oSupport,
    sType = "Civilian"
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetCargo("Stingray II")
  oSupport:SetDeliveryVehicle("Mi26 (PMC) (Driver)")
  tSupportData.stingrayii = {
    sName = "[vehicle.stingrayii]",
    sDescription = "[support.vehicle.stingrayii.desc]",
    sIcon = "vehicles_tank_stingray2",
    nMaxStock = 99,
    nCashCost = 50000,
    nFuelCost = 140,
    oSupport = oSupport,
    sType = "Heavy"
  }
  oSupport = mrxstrategicmissile:Create()
  tSupportData.strategicmissile = {
    sName = "[support.airstrike.strategicmissile.name]",
    sDescription = "[support.airstrike.strategicmissile.desc]",
    sIcon = "support_strategic_missle",
    nMaxStock = 99,
    nCashCost = 400000,
    nFuelCost = 220,
    oSupport = oSupport,
    sType = "Airstrike"
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetCargo("Supply Drop (Support)")
  oSupport:SetCareless(true)
  tSupportData.Support = {
    sName = "[support.supply.Support.name]",
    sDescription = "[support.supply.Support.desc]",
    sIcon = "supplies_rpg",
    nMaxStock = 99,
    nCashCost = 5000,
    nFuelCost = 40,
    oSupport = oSupport,
    sType = "Supply"
  }
  oSupport = mrxsurgicalstrike:Create()
  tSupportData.surgicalstrike = {
    sName = "[support.airstrike.surgicalstrike.name]",
    sDescription = "[support.airstrike.surgicalstrike.desc]",
    sIcon = "support_surgical_strike",
    nMaxStock = 99,
    nCashCost = 350000,
    nFuelCost = 280,
    oSupport = oSupport,
    sType = "Airstrike"
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetCargo("SX2150 (MLRS)")
  tSupportData.sx2150mlrs = {
    sName = "[vehicle.sx2150mlrs]",
    sDescription = "[support.vehicle.sx2150mlrs.desc]",
    sIcon = "vehicles_truck_sx2150",
    nMaxStock = 99,
    nCashCost = 60000,
    nFuelCost = 120,
    oSupport = oSupport,
    sType = "Light"
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetCargo("T300 (M60)")
  tSupportData.t300m60 = {
    sName = "[vehicle.t300m60]",
    sDescription = "[support.vehicle.t300m60.desc]",
    sIcon = "vehicles_truck_t300",
    nMaxStock = 99,
    nCashCost = 20000,
    nFuelCost = 60,
    oSupport = oSupport,
    sType = "Light"
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetCargo("Tank Bike")
  tSupportData.tankbike = {
    sName = "[vehicle.tankbike]",
    sDescription = "[support.vehicle.tankbike.desc]",
    sIcon = "vehicles_motorcycle_tankbike",
    nMaxStock = 99,
    nCashCost = 750000,
    nFuelCost = 60,
    oSupport = oSupport,
    sType = "Light"
  }
  oSupport = mrxtankbuster:Create()
  tSupportData.tankbuster = {
    sName = "[support.airstrike.tankbuster.name]",
    sDescription = "[support.airstrike.tankbuster.desc]",
    sIcon = "support_tank_buster",
    nMaxStock = 99,
    nCashCost = 100000,
    nFuelCost = 200,
    oSupport = oSupport,
    sType = "Airstrike"
  }
  oSupport = mrxboatdelivery:Create()
  oSupport:SetCargo("Turbosquid (GR)")
  oSupport:SetCareless(true)
  tSupportData.turbosquidgr = {
    sName = "[vehicle.turbosquidgr]",
    sDescription = "[support.vehicle.turbosquidgr.desc]",
    sIcon = "vehicles_boat_turbosquid",
    nMaxStock = 99,
    nCashCost = 5000,
    nFuelCost = 40,
    oSupport = oSupport,
    sType = "Boat"
  }
  oSupport = mrxboatdelivery:Create()
  oSupport:SetCargo("Turbosquid (OC)")
  oSupport:SetCareless(true)
  tSupportData.turbosquidoc = {
    sName = "[vehicle.turbosquidoc]",
    sDescription = "[support.vehicle.turbosquidoc.desc]",
    sIcon = "vehicles_boat_turbosquid",
    nMaxStock = 99,
    nCashCost = 5000,
    nFuelCost = 40,
    oSupport = oSupport,
    sType = "Boat"
  }
  oSupport = mrxsupportcopterdelivery:Create()
  oSupport:SetDeliveryVehicle("UH1 Transport (GR) (Ewan)")
  tSupportData.uh1transportgr = {
    sName = "[vehicle.uh1transportgr]",
    sDescription = "[support.vehicle.uh1transportgr.desc]",
    sIcon = "vehicles_heli_uh1",
    nMaxStock = 99,
    nCashCost = 40000,
    nFuelCost = 80,
    oSupport = oSupport,
    sType = "Heli"
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetCargo({
    "Van (Commercial)",
    "Van (Green)",
    "Van (Racing)",
    "Van (Taxi)",
    "Garbage Truck",
    "Impact",
    "Impact (SUT)",
    "Escort",
    "Ambulance",
    "Armored Bank Truck",
    "Transport Truck"
  })
  tSupportData.utility = {
    sName = "[vehicle.utility]",
    sDescription = "[support.vehicle.utility.desc]",
    sIcon = "vehicles_car_van",
    nMaxStock = 99,
    nCashCost = 20000,
    nFuelCost = 60,
    oSupport = oSupport,
    sType = "Civilian"
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetCargo("Valiant (Python)")
  tSupportData.valiantpython = {
    sName = "[vehicle.valiantpython]",
    sDescription = "[support.vehicle.valiantpython.desc]",
    sIcon = "vehicles_car_valiant",
    nMaxStock = 99,
    nCashCost = 50000,
    nFuelCost = 60,
    oSupport = oSupport,
    sType = "Civilian"
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetCargo("Veyron (Assault)")
  tSupportData.veyronassault = {
    sName = "[vehicle.veyronassault]",
    sDescription = "[support.vehicle.veyronassault.desc]",
    sIcon = "vehicles_light_urban_commando",
    nMaxStock = 99,
    nCashCost = 1000000,
    nFuelCost = 60,
    oSupport = oSupport,
    sType = "Light"
  }
  oSupport = mrxsupportcopterdelivery:Create()
  oSupport:SetDeliveryVehicle("WZ10 (Ewan)")
  tSupportData.wz10 = {
    sName = "[vehicle.wz10]",
    sDescription = "[support.vehicle.wz10.desc]",
    sIcon = "vehicles_heli_wz10",
    nMaxStock = 99,
    nCashCost = 350000,
    nFuelCost = 200,
    oSupport = oSupport,
    sType = "Heli"
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetCargo("WZ551")
  oSupport:SetDeliveryVehicle("Mi26 (PMC) (Driver)")
  tSupportData.wz551 = {
    sName = "[vehicle.wz551]",
    sDescription = "[support.vehicle.wz551.desc]",
    sIcon = "vehicles_apc_wz551",
    nMaxStock = 99,
    nCashCost = 70000,
    nFuelCost = 160,
    oSupport = oSupport,
    sType = "Heavy"
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetCargo("ZBD2000")
  oSupport:SetDeliveryVehicle("Mi26 (PMC) (Driver)")
  tSupportData.zbd2000 = {
    sName = "[vehicle.zbd2000]",
    sDescription = "[support.vehicle.zbd2000.desc]",
    sIcon = "vehicles_apc_zbd2000",
    nMaxStock = 99,
    nCashCost = 85000,
    nFuelCost = 160,
    oSupport = oSupport,
    sType = "Heavy"
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetCargo("ZTZ63a")
  oSupport:SetDeliveryVehicle("Mi26 (PMC) (Driver)")
  tSupportData.ztz63a = {
    sName = "[vehicle.ztz63a]",
    sDescription = "[support.vehicle.ztz63a.desc]",
    sIcon = "vehicles_tank_ztz63",
    nMaxStock = 99,
    nCashCost = 100000,
    nFuelCost = 180,
    oSupport = oSupport,
    sType = "Heavy"
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetCargo("ZTZ98")
  oSupport:SetDeliveryVehicle("Mi26 (PMC) (Driver)")
  tSupportData.ztz98 = {
    sName = "[vehicle.ztz98]",
    sDescription = "[support.vehicle.ztz98.desc]",
    sIcon = "vehicles_tank_ztz98",
    nMaxStock = 99,
    nCashCost = 425000,
    nFuelCost = 220,
    oSupport = oSupport,
    sType = "Heavy"
  }
  oSupport = mrxbunkerbuster:Create()
  oSupport:SetBomb("Nuclear Bunker Buster Projectile")
  tSupportData.nuke = {
    sName = "[AllCon003.Terms.Reward]",
    sDescription = "[support.nuke.desc]",
    sIcon = "support_bunker_buster",
    nMaxStock = 99,
    nCashCost = 1000000,
    nFuelCost = 500,
    oSupport = oSupport,
    sType = "Airstrike"
  }
  oSupport = mrxclusterbomb:Create()
  oSupport:SetRecruit("Fiona")
  oSupport:SetDeliveryVehicle("Support Vehicle (OV10) low altitude")
  tSupportData.upclusterbomb = {
    sName = "[support.airstrike.upclusterbomb.name]",
    sDescription = "[support.airstrike.clusterbomb.desc]",
    sIcon = "support_cluster_bomb",
    nMaxStock = 99,
    nCashCost = 500000,
    nFuelCost = 180,
    oSupport = oSupport,
    sType = "Airstrike"
  }
  oSupport = mrxtankbuster:Create()
  oSupport:SetRecruit("Fiona")
  oSupport:SetDeliveryVehicle("Support Vehicle (OV10) low altitude")
  tSupportData.uptankbuster = {
    sName = "[support.airstrike.uptankbuster.name]",
    sDescription = "[support.airstrike.tankbuster.desc]",
    sIcon = "support_tank_buster",
    nMaxStock = 99,
    nCashCost = 350000,
    nFuelCost = 180,
    oSupport = oSupport,
    sType = "Airstrike"
  }
  oSupport = mrxcombatairpatrol:Create()
  oSupport:SetRecruit("Fiona")
  oSupport:SetDeliveryVehicle("Support Vehicle (OV10) low altitude")
  tSupportData.upcombatairpatrol = {
    sName = "[support.airstrike.upcap.name]",
    sDescription = "[support.airstrike.combatairpatrol.desc]",
    sIcon = "support_combat_air_patrol",
    nMaxStock = 99,
    nCashCost = 400000,
    nFuelCost = 200,
    oSupport = oSupport,
    sType = "Airstrike"
  }
  oSupport = mrxsupportcopterdelivery:Create()
  oSupport:SetDeliveryVehicle("Ka29b (Ewan)")
  tFreebieData.ChiCon001_Copter = {
    sName = "[vehicle.ka29b]",
    sIcon = "vehicles_heli_ka28",
    nFreebieQty = 1,
    oSupport = oSupport
  }
  oSupport = mrxsurgicalstrike:Create()
  oSupport:SetDeliveryVehicle("Support Vehicle (Q5)")
  tFreebieData.ChiCon001_Airstrike = {
    sName = "[support.airstrike.surgicalstrike.name]",
    sIcon = "support_surgical_strike",
    nFreebieQty = 3,
    oSupport = oSupport
  }
  oSupport = mrxrocketartillery:Create()
  tFreebieData.ChiCon001_RocketArtillery = {
    sName = "[support.airstrike.rocketartillery.name]",
    sIcon = "support_rocket_artillery",
    nFreebieQty = 3,
    oSupport = oSupport
  }
  oSupport = mrxgunship:Create()
  tFreebieData.Gunship = {
    sName = "[support.airstrike.gunship.name]",
    sIcon = "vehicles_plane_c130",
    nFreebieQty = 1,
    oSupport = oSupport
  }
  oSupport = mrxlaserguidedbomb:Create()
  oSupport:SetBomb("Practice LGB Projectile")
  tFreebieData["Practice Laser"] = {
    sName = "[support.airstrike.laserguidedbomb.name]",
    sIcon = "support_laser_guided_bomb",
    nFreebieQty = 2,
    oSupport = oSupport
  }
  oSupport = mrxsatelliteguidedbomb:Create()
  oSupport:SetCost(0)
  oSupport:SetBomb("Practice LGB Projectile")
  tFreebieData["Practice Satellite"] = {
    sName = "[support.airstrike.satelliteguidedbomb.name]",
    sIcon = "support_satellite_guided_bomb",
    nFreebieQty = 1,
    oSupport = oSupport
  }
  oSupport = mrxsatelliteguidedbomb:Create()
  oSupport:SetCost(0)
  oSupport:SetDeliveryVehicle("Support Vehicle (Tucano)")
  tFreebieData.VzaCon01_SatBomb = {
    sName = "[support.airstrike.satelliteguidedbomb.name]",
    sIcon = "support_satellite_guided_bomb",
    nFreebieQty = 1,
    oSupport = oSupport
  }
  oSupport = mrxclusterbomb:Create()
  oSupport:SetDeliveryVehicle("Support Vehicle (Tucano)")
  tFreebieData.VzaCon001_Airstrike = {
    sName = "[support.airstrike.clusterbomb.name]",
    sIcon = "support_cluster_bomb",
    nFreebieQty = 3,
    oSupport = oSupport
  }
  oSupport = mrxclusterbomb:Create()
  oSupport:SetDeliveryVehicle("Support Vehicle (OV10) low altitude")
  tFreebieData.OC_ClusterBomb = {
    sName = "[support.airstrike.clusterbomb.name]",
    sIcon = "support_cluster_bomb",
    nFreebieQty = 1,
    oSupport = oSupport
  }
  oSupport = mrxbombingrun:Create()
  oSupport:SetDeliveryVehicle("Support Vehicle (OV10) low altitude")
  tFreebieData.OC_BombingRun = {
    sName = "[support.airstrike.bombingrun.name]",
    sIcon = "support_bombing_run",
    nFreebieQty = 1,
    oSupport = oSupport
  }
  oSupport = mrxartillery:Create()
  oSupport:SetDeliveryVehicle("Guerilla Soldier")
  tFreebieData.GurCon001_Artillery = {
    sName = "[support.airstrike.artillery.name]",
    sIcon = "support_artillery",
    nFreebieQty = 1,
    oSupport = oSupport
  }
  oSupport = mrxartillery:Create()
  oSupport:SetDeliveryVehicle("Guerilla Soldier")
  tFreebieData.GurCon002_Artillery = {
    sName = "[support.airstrike.artillery.name]",
    sIcon = "support_artillery",
    nFreebieQty = 3,
    oSupport = oSupport
  }
  oSupport = mrxlaserguidedbomb:Create()
  oSupport:SetDeliveryVehicle("Support Vehicle (Q5)")
  tFreebieData.ChiCon002_Bombs = {
    sName = "[support.airstrike.laserguidedbomb.name]",
    sIcon = "support_laser_guided_bomb",
    nFreebieQty = 4,
    oSupport = oSupport
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetCargo("Supply Drop (AA)")
  oSupport:SetDeliveryVehicle("Coanda Transport (Driver)")
  oSupport:SetCareless(true)
  tFreebieData.OilCon001_Crate = {
    sName = "[support.supply.aa.name]",
    sDescription = "[support.supply.aa.desc]",
    sIcon = "HUD_ICON_support_crate",
    nFreebieQty = 4,
    oSupport = oSupport
  }
  oSupport = mrxbunkerbuster:Create()
  oSupport:SetBomb("Nuclear Bunker Buster Projectile")
  tFreebieData.PmcCon004_Nuke = {
    sName = "[AllCon003.Terms.Reward]",
    sIcon = "support_bunker_buster",
    nFreebieQty = 1,
    oSupport = oSupport
  }
  oSupport = mrxbunkerbuster:Create()
  tFreebieData["Bunker Buster"] = {
    sName = "[support.airstrike.bunkerbuster.name]",
    sIcon = "support_bunker_buster",
    nFreebieQty = 1,
    oSupport = oSupport
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetCargo("_global_ramp_roadlessrider")
  tFreebieData["Ramp Delivery"] = {
    sName = "TEMP: Ramp Delivery",
    sIcon = "vehicles_heli_uh1",
    nFreebieQty = 10,
    oSupport = oSupport
  }
  oSupport = MrxOilCon002Delivery:Create()
  tFreebieData.OilCon002_Delivery = {
    sName = "[support.supply.listeningpost.name]",
    sIcon = "vehicles_heli_uh1",
    nFreebieQty = nil,
    oSupport = oSupport
  }
  oSupport = MrxMunitionsPickup:Create()
  oSupport:SetDeliveryVehicle("UH1 Transport (GR) (Driver)")
  oSupport:SetFinalDestination("05_gur_hq_lz_playerone")
  tFreebieData.GurCon001_Munitions = {
    sName = "[support.munition.name]",
    sIcon = "vehicles_heli_uh1",
    nFreebieQty = nil,
    oSupport = oSupport
  }
  oSupport = MrxMunitionsPickup:Create()
  oSupport:SetDeliveryVehicle("UH1 Transport (PMC) (Ghost)")
  oSupport:SetFinalDestination("01_pmc_hq_lz_playerone")
  tFreebieData.MunitionsPickup = {
    sName = "[support.munition.name]",
    sIcon = "vehicles_heli_uh1",
    nFreebieQty = nil,
    oSupport = oSupport
  }
  oSupport = MrxSupportPickup:Create()
  oSupport:SetPickupVehicle("UH1 Transport (PMC) (Extraction)")
  oSupport:SetFinalDestination("01_pmc_hq_lz_playerone")
  tFreebieData.Extraction_PMC = {
    sName = "[support.extraction.name]",
    sIcon = "vehicles_heli_uh1",
    nFreebieQty = nil,
    oSupport = oSupport
  }
  oSupport = MrxSupportPickup:Create()
  oSupport:SetPickupVehicle("MH53J (Extraction)")
  oSupport:SetFinalDestination("07_all_hq_lz_playerone")
  tFreebieData.Extraction_AL = {
    sName = "[support.extraction.name]",
    sIcon = "vehicles_heli_uh1",
    nFreebieQty = nil,
    oSupport = oSupport
  }
  oSupport = MrxSupportPickup:Create()
  oSupport:SetPickupVehicle("Ka29b (Extraction)")
  oSupport:SetFinalDestination("12_chi_hq_lz_playerone")
  tFreebieData.Extraction_CH = {
    sName = "[support.extraction.name]",
    sIcon = "vehicles_heli_ka28",
    nFreebieQty = nil,
    oSupport = oSupport
  }
  oSupport = MrxSupportPickup:Create()
  oSupport:SetPickupVehicle("UH1 Transport (GR) (Extraction)")
  oSupport:SetFinalDestination("05_gur_hq_lz_playerone")
  tFreebieData.Extraction_GR = {
    sName = "[support.extraction.name]",
    sIcon = "vehicles_heli_uh1",
    nFreebieQty = nil,
    oSupport = oSupport
  }
  oSupport = MrxSupportPickup:Create()
  oSupport:SetPickupVehicle("Coanda Transport (Extraction)")
  oSupport:SetFinalDestination("02_oil_hq_lz_playerone")
  tFreebieData.Extraction_OC = {
    sName = "[support.extraction.name]",
    sIcon = "vehicles_heli_md500",
    nFreebieQty = nil,
    oSupport = oSupport
  }
  oSupport = MrxSupportPickup:Create()
  oSupport:SetPickupVehicle("Alouette3 Transport (PR) (Extraction)")
  oSupport:SetFinalDestination("08_pir_hq_lz_playerone")
  tFreebieData.Extraction_PR = {
    sName = "[support.extraction.name]",
    sIcon = "vehicles_heli_alouette",
    nFreebieQty = nil,
    oSupport = oSupport
  }
  oSupport = MrxSoldierDelivery:Create()
  oSupport:SetDeliveryVehicle("MH53J (Full)")
  oSupport:SetFinalDestination("07_all_hq_lz_playerone")
  tFreebieData.SoldierDelivery_AL = {
    sName = "[support.soldierdelivery.al.name]",
    sIcon = "vehicles_heli_uh1",
    oSupport = oSupport
  }
  oSupport = MrxSoldierDelivery:Create()
  oSupport:SetDeliveryVehicle("Ka29b (Full)")
  oSupport:SetFinalDestination("12_chi_hq_lz_playerone")
  tFreebieData.SoldierDelivery_CH = {
    sName = "[support.soldierdelivery.ch.name]",
    sIcon = "vehicles_heli_ka28",
    oSupport = oSupport
  }
  oSupport = MrxSoldierDelivery:Create()
  oSupport:SetDeliveryVehicle("UH1 Transport (GR) (Full)")
  oSupport:SetFinalDestination("05_gur_hq_lz_playerone")
  tFreebieData.SoldierDelivery_GR = {
    sName = "[support.soldierdelivery.gr.name]",
    sIcon = "vehicles_heli_uh1",
    oSupport = oSupport
  }
  oSupport = MrxSoldierDelivery:Create()
  oSupport:SetDeliveryVehicle("Coanda Transport (Full)")
  oSupport:SetFinalDestination("02_oil_hq_lz_playerone")
  tFreebieData.SoldierDelivery_OC = {
    sName = "[support.soldierdelivery.oc.name]",
    sIcon = "vehicles_heli_md500",
    oSupport = oSupport
  }
  oSupport = MrxSoldierDelivery:Create()
  oSupport:SetDeliveryVehicle("Alouette3 Transport (PR) (Full)")
  oSupport:SetFinalDestination("08_pir_hq_lz_playerone")
  tFreebieData.SoldierDelivery_PR = {
    sName = "[support.soldierdelivery.pr.name]",
    sIcon = "vehicles_heli_alouette",
    oSupport = oSupport
  }
  oSupport = mrxsupportcopterdelivery:Create()
  oSupport:SetDeliveryVehicle("AH1Z (Ewan)")
  oSupport:SetFinalDestination("01_pmc_hq_lz_playerone")
  tFreebieData.CopterDelivery_AL = {
    sName = "Allied Copter Delivery",
    sIcon = "vehicles_heli_alouette",
    oSupport = oSupport
  }
  oSupport = mrxsupportcopterdelivery:Create()
  oSupport:SetDeliveryVehicle("WZ10 (Ewan)")
  oSupport:SetFinalDestination("01_pmc_hq_lz_playerone")
  tFreebieData.CopterDelivery_CH = {
    sName = "Chinese Copter Delivery",
    sIcon = "vehicles_heli_alouette",
    oSupport = oSupport
  }
  oSupport = mrxsupportcopterdelivery:Create()
  oSupport:SetDeliveryVehicle("Coanda Superiority (Ewan)")
  oSupport:SetFinalDestination("01_pmc_hq_lz_playerone")
  tFreebieData.CopterDelivery_OC = {
    sName = "UP Copter Delivery",
    sIcon = "vehicles_heli_alouette",
    oSupport = oSupport
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetDeliveryVehicle("Coanda Transport (Driver)")
  oSupport:SetCareless(true)
  oSupport:SetCargo("Supply Drop (Light MG)")
  oSupport:SetFinalDestination("02_oil_hq_lz_playerone")
  tFreebieData.LightMG = {
    sName = "[support.supply.lightmg.name]",
    sIcon = "HUD_ICON_support_crate",
    nFreebieQty = 1,
    oSupport = oSupport
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetDeliveryVehicle("Coanda Transport (Driver)")
  oSupport:SetCargo("Supply Drop (OC)")
  oSupport:SetFinalDestination("02_oil_hq_lz_playerone")
  oSupport:SetCareless(true)
  tFreebieData.OC = {
    sName = "[support.supply.oc.name]",
    sIcon = "HUD_ICON_support_crate",
    nFreebieQty = 2,
    oSupport = oSupport
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetDeliveryVehicle("Coanda Transport (Driver)")
  oSupport:SetCargo("Supply Drop (OC)")
  oSupport:SetFinalDestination("02_oil_hq_lz_playerone")
  oSupport:SetCareless(true)
  tFreebieData.OC = {
    sName = "[support.supply.oc.name]",
    sIcon = "HUD_ICON_support_crate",
    nFreebieQty = 2,
    oSupport = oSupport
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetDeliveryVehicle("UH1 Transport (PMC) (Driver)")
  oSupport:SetCargo("Supply Drop (OC)")
  oSupport:SetFinalDestination("02_oil_hq_lz_playerone")
  oSupport:SetCareless(true)
  tFreebieData.OilCon002_OC = {
    sName = "[support.supply.oc.name]",
    sIcon = "HUD_ICON_support_crate",
    nFreebieQty = 2,
    oSupport = oSupport
  }
  oSupport = mrxcratedelivery:Create()
  oSupport:SetDeliveryVehicle("UH1 Transport (PMC) (Driver)")
  oSupport:SetCargo("EXT")
  oSupport:SetFinalDestination("02_oil_hq_lz_playerone")
  tFreebieData.OilCon002_EXT = {
    sName = "[vehicle.ext]",
    sIcon = "vehicles_truck_ext",
    nFreebieQty = 1,
    oSupport = oSupport
  }
  oSupport = mrxcruisemissile:Create()
  oSupport:SetDeliveryVehicle("Allied Soldier")
  tFreebieData.AL_CruiseMissile = {
    sName = "[support.airstrike.cruisemissile.name]",
    sIcon = "support_cruise_missle",
    nFreebieQty = 1,
    oSupport = oSupport
  }
  oSupport = mrxcruisemissile:Create()
  oSupport:SetDeliveryVehicle("Chinese Soldier")
  tFreebieData.CH_CruiseMissile = {
    sName = "[support.airstrike.cruisemissile.name]",
    sIcon = "support_cruise_missle",
    nFreebieQty = 1,
    oSupport = oSupport
  }
  oSupport = mrxdaisycutter:Create()
  oSupport:SetDeliveryVehicle("Support Vehicle (C130)")
  tFreebieData.AL_DaisyCutter = {
    sName = "[support.airstrike.daisycutter.name]",
    sIcon = "support_daisy_cutter",
    nFreebieQty = 1,
    oSupport = oSupport
  }
  oSupport = mrxlaserguidedbomb:Create()
  oSupport:SetDeliveryVehicle("Support Vehicle (F35)")
  tFreebieData.AL_LaserGuidedBomb = {
    sName = "[support.airstrike.laserguidedbomb.name]",
    sIcon = "support_laser_guided_bomb",
    nFreebieQty = 4,
    oSupport = oSupport
  }
  oSupport = mrxgunship:Create()
  tFreebieData.AL_Gunship = {
    sName = "[support.airstrike.gunship.name]",
    sIcon = "support_gunship",
    oSupport = oSupport
  }
  for sName, tData in pairs(tSupportData) do
    if tData.oSupport then
      tData.oSupport:SetSupportName(sName)
      if tData.nMaxStock then
        tData.nMaxStock = _kMaxStock
      end
    end
  end
end

function GetFreebie(sSupportName)
  return tFreebieData[sSupportName]
end

NETEVENT_ADDFREEBIE = 0
NETEVENT_REMOVEFREEBIE = 1

function AddAllFreebies()
  for sFreebieName, tData in pairs(tFreebieData) do
    AddFreebie(sFreebieName, 1, nil, true)
  end
end

function GetLocalRemote(vPlayers)
  local bLocal, bRemote, tPlayers
  if vPlayers then
    if "table" == type(vPlayers) then
      tPlayers = vPlayers
    elseif "userdata" == type(vPlayers) then
      tPlayers = {vPlayers}
    else
      return false, false
    end
    for nIndex, uGuid in pairs(tPlayers) do
      if Player.IsLocal(uGuid) then
        bLocal = true
      else
        bRemote = true
      end
    end
  else
    bLocal = true
    bRemote = true
  end
  return bLocal, bRemote
end

function _AddFreebie(sSupportName, nQty, bAddingAllFreebies, nMaxQty)
  if not tFreebieData[sSupportName] then
    Debug.Printf("MrxSupportData: " .. tostring(sSupportName) .. " not found.")
    return
  end
  tFreebieData[sSupportName].bDontNetSync = bAddingAllFreebies
  local sName = tFreebieData[sSupportName].sName
  if bAddingAllFreebies then
    if sSupportName == "OilCon002_Delivery" then
      MrxOilCon002Delivery.ResetDropZones()
    end
    sName = sSupportName
  end
  if tFreebieData[sSupportName].bActive then
    nQty = nQty or 1
    local nCurrent = MrxPmc.GetFreebieQty(sName)
    if nCurrent then
      if nMaxQty and nMaxQty < nCurrent + nQty then
        Debug.Printf("AddFreebie(): Giving " .. tostring(nMaxQty - nCurrent) .. sName .. " instead of " .. tostring(nQty) .. " to meet given limit of " .. tostring(nMaxQty))
        nQty = nMaxQty - nCurrent
      end
      MrxPmc.SetFreebieQty(sName, nCurrent + nQty)
      tFreebieData[sSupportName].nInitialQty = nQty
    else
      Debug.Printf("AddFreeibie(): bActive true but GetFreebieQty() returns nil!")
    end
  else
    local oSupport = tFreebieData[sSupportName].oSupport
    oSupport:SetSupportName(sName)
    nQty = nQty or tFreebieData[sSupportName].nFreebieQty
    if nQty then
      if nMaxQty and nMaxQty < nQty then
        Debug.Printf("AddFreebie(): Initial add quantity too high: " .. tostring(nQty) .. ", limit is " .. tostring(nMaxQty))
        nQty = nMaxQty
      end
      MrxPmc.SetFreebieQty(sName, nQty)
      tFreebieData[sSupportName].nInitialQty = nQty
    else
      Debug.Printf("AddFreebie(): Could not get a quantity to add for " .. sName)
    end
    Hud.SupportMenu:AddItem({
      vPlayer = nil,
      sName = sName,
      sIcon = tFreebieData[sSupportName].sIcon,
      oSupport = oSupport,
      bDontNetSync = true,
      bAnimate = true
    })
    tFreebieData[sSupportName].bActive = true
  end
end

function _RemoveFreebie(sSupportName)
  if not tFreebieData[sSupportName] then
    Debug.Printf("MrxSupportData: " .. tostring(sSupportName) .. " not found.")
    return
  end
  Hud.SupportMenu:RemoveItem({
    vPlayer = nil,
    sName = tFreebieData[sSupportName].sName,
    bDontNetSync = true
  })
  MrxPmc.SetFreebieQty(tFreebieData[sSupportName].sName, nil)
  tFreebieData[sSupportName].bActive = false
end

function AddFreebie(sSupportName, nQty, vPlayers, bAddingAllFreebies, nMaxQty)
  local bLocal, bRemote = GetLocalRemote(vPlayers)
  if bRemote and Net.IsServer() and bAddingAllFreebies == nil then
    Net.SendCustomEvent("MrxSupportData", NETEVENT_ADDFREEBIE, {
      sSupportName,
      nQty or 0,
      nMaxQty
    })
  end
  if bLocal then
    _AddFreebie(sSupportName, nQty, bAddingAllFreebies, nMaxQty)
  end
end

function RemoveFreebie(sSupportName, vPlayers)
  local bLocal, bRemote = GetLocalRemote(vPlayers)
  if bRemote and Net.IsServer() then
    Net.SendCustomEvent("MrxSupportData", NETEVENT_REMOVEFREEBIE, {sSupportName})
  end
  if bLocal then
    _RemoveFreebie(sSupportName)
  end
end

function GetFreebieStringIndex(uStringHash)
  Debug.Printf("Looking for freebie with stringhash = " .. tostring(uStringHash))
  for sFreebieName, tData in pairs(tFreebieData) do
    if String.GetHash(sFreebieName) == uStringHash then
      return sFreebieName
    end
  end
  Debug.Printf("couldn't find it")
  return "NO NAME"
end

function GetSupportStringIndex(uStringHash)
  for sSupportName, tData in pairs(tSupportData) do
    if String.GetHash(sSupportName) == uStringHash then
      return sSupportName
    end
  end
end

function NetEventCallback(nEventId, tArgs)
  local sSupportName = GetFreebieStringIndex(tArgs[1])
  if nEventId == NETEVENT_ADDFREEBIE then
    if tArgs[2] > 0 then
      _AddFreebie(sSupportName, tArgs[2], nil, tArgs[3])
    else
      _AddFreebie(sSupportName)
    end
  elseif nEventId == NETEVENT_REMOVEFREEBIE then
    _RemoveFreebie(sSupportName)
  end
end

_knUnlockStatusNew = 1
_knUnlockStatusViewed = 2

function Add(tSupport, sFaction)
  for _, sName in ipairs(tSupport) do
    local tItem = tSupportData[sName]
    if tItem then
      tItem.tUnlockStatus = tItem.tUnlockStatus or {}
      if not tItem.tUnlockStatus[sFaction] then
        tItem.tUnlockStatus[sFaction] = _knUnlockStatusNew
        WifMissionFlow.RefreshAllPdaMissionDetails()
      end
    end
  end
  local tFactionAbbrevs = MrxFactionManager.GetFactionAbbrevs()
  local nAllShopsUnlocked = 0
  local nAllShopsTotal = 0
  for i, sFactionAbbrev in ipairs(tFactionAbbrevs) do
    local nTotal = MrxShop.GetTotalNumberOfItems(sFactionAbbrev)
    local nUnlocked = MrxShop.GetNumberOfUnlockedItems(sFactionAbbrev)
    if 0 < nTotal then
      nAllShopsUnlocked = nAllShopsUnlocked + nUnlocked
      nAllShopsTotal = nAllShopsTotal + nTotal
    end
  end
  if nAllShopsUnlocked == nAllShopsTotal then
    MrxAchievements.NetGrantAchievement("ACHIEVEMENT_DIGITAL_MAN", Player.GetPrimaryPlayer())
  end
end

function IsItemUnlocked(sSupportId, sFaction)
  local tItem = tSupportData[sSupportId]
  if tItem.tUnlockStatus then
    return tItem.tUnlockStatus[sFaction] ~= nil
  end
  return false
end

function IsItemNew(sSupportId, sFaction)
  local tItem = tSupportData[sSupportId]
  if tItem.tUnlockStatus then
    return tItem.tUnlockStatus[sFaction] == _knUnlockStatusNew
  end
  return false
end

function SetItemViewed(sSupportId, sFaction)
  local tItem = tSupportData[sSupportId]
  if tItem.tUnlockStatus and tItem.tUnlockStatus[sFaction] == _knUnlockStatusNew then
    tItem.tUnlockStatus[sFaction] = _knUnlockStatusViewed
  end
end

function SaveSingleton()
  local tSaveData = {}
  for sName, tData in pairs(tSupportData) do
    tSaveData[sName] = {
      tUnlockStatus = tData.tUnlockStatus,
      nGlobalStock = tData.nGlobalStock
    }
  end
  tSaveData._tRequirementsObtained = tRequirementsObtained
  return tSaveData
end

function LoadSingleton(tSaveData)
  if not tSaveData then
    return
  end
  tRequirementsObtained = tSaveData._tRequirementsObtained
  tSaveData._tRequirementsObtained = nil
  for sName, tData in pairs(tSupportData) do
    if tSaveData[sName] then
      tData.tUnlockStatus = tSaveData[sName].tUnlockStatus
      tData.nGlobalStock = tSaveData[sName].nGlobalStock
    end
  end
end

function AddSupportData(tSupportDataToAdd, sKey)
  if not g_bIsDlc then
    Debug.Printf("@@@@@@@@@ MrxSupportData: AddSupportData returned nil!!! ")
    return nil
  end
  if tSupportDataToAdd == nil then
    return nil
  end
  tSupportData[sKey] = tSupportDataToAdd
  return true
end

function GetMaxQuantity()
  return _kMaxStock
end

function GetPlayerVisibleName(sSupportId)
  local tItem = tSupportData[sSupportId]
  if tItem then
    return tItem.sName
  end
end

function GetFreebieName(sSupportId)
  local tItem = tFreebieData[sSupportId]
  if tItem then
    return tItem.sName
  end
end
