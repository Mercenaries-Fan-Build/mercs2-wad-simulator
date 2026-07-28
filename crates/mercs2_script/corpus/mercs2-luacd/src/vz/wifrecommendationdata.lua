import("MrxFactionManager")
import("MrxPmc")
import("MrxRewardData")
import("MrxShop")
import("MrxSupportData")
_tRecommendations = {
  OilJob008 = {c4 = 3},
  OilCon001 = {
    gl = 1,
    extgl = 1,
    upcombatairpatrol = 2
  },
  OilCon051 = {c4 = 1},
  OilCon003 = {extgl = 1},
  OilCon052 = {
    uptankbuster = 1,
    artillery = 3,
    stingrayii = 1
  },
  GurJob020 = {artillery = 3},
  GurCon002 = {
    uptankbuster = 3,
    c4 = 2,
    upcombatairpatrol = 1
  },
  GurCon001 = {
    artillery = 4,
    piranha = 2,
    upcombatairpatrol = 2,
    c4 = 3
  },
  GurCon050 = {uptankbuster = 1, c4 = 1},
  GurCon005 = {sniperch = 1, coandaattack = 1},
  GurCon052 = {daisycutter = 1, endriagoattack = 1},
  PmcCon002 = {
    artillery = 3,
    patrolboatvz = 1,
    alouette3transportvz = 1,
    sniperru = 2,
    c4 = 2,
    pr = 1
  },
  PirCon051 = {gl = 1, daisycutter = 1},
  PirCon052 = {
    artillery = 1,
    patrolboatvz = 1,
    c4 = 1,
    strategicmissile = 1
  },
  JetCon001 = {
    mi35 = 2,
    patrolboatvz = 2,
    artillery = 2
  },
  PmcCon003 = {
    alouette3superiority = 2,
    combatairpatrol = 2,
    tankbuster = 2
  },
  AllJob020 = {surgicalstrike = 3},
  AllCon002 = {
    smartbomb = 3,
    laserguidedbomb = 3,
    laviiimgs = 1
  },
  AllCon001 = {
    laserguidedbomb = 3,
    carpetbomb = 1,
    wz10 = 2,
    atal = 2,
    dinghy = 2
  },
  AllCon003 = {
    moab = 1,
    tankbuster = 2,
    laserguidedbomb = 2,
    surgicalstrike = 3
  },
  ChiJob020 = {fuelairbomb = 3},
  ChiCon001 = {
    rocketartillery = 3,
    mh53j = 1,
    cruisemissile = 3,
    atch = 2
  },
  ChiCon002 = {
    carpetbomb = 2,
    strategicmissile = 3,
    fuelairbomb = 5,
    cruisemissile = 3,
    atch = 3
  },
  ChiCon003 = {fuelairbomb = 3, smartbomb = 3}
}

function HasRecommendations(sMissionId)
  return _tRecommendations[sMissionId] ~= nil
end

function GenerateRecommendationString(sMissionId)
  local tRecData = _tRecommendations[sMissionId]
  if not tRecData then
    return
  end
  local sOutput = ""
  local bAllInStock = true
  for sSupportId, nQty in pairs(tRecData) do
    local sLineItem, bInStock = _FormatLineItem(sSupportId, nQty)
    if bInStock ~= nil and bAllInStock then
      bAllInStock = bInStock
    end
    if sLineItem then
      sOutput = sOutput .. sLineItem .. "\n"
    end
  end
  return sOutput, bAllInStock
end

function _FormatLineItem(sSupportId, nQty)
  local sOutput = MrxSupportData.GetPlayerVisibleName(sSupportId)
  if not sOutput then
    return
  end
  local tSupportData = MrxSupportData.tSupportData[sSupportId]
  if not tSupportData then
    return
  end
  local bTypeIconPrepended = false
  if tSupportData.sType then
    local tTypeToMarkupCode = {
      Airstrike = "[airstrike]",
      Supply = "[supply]",
      Light = "[vehmlight]",
      Heavy = "[vehmheavy]",
      Civilian = "[vehcivilian]",
      Boat = "[vehboat]",
      Heli = "[vehheli]"
    }
    local sMarkupCode = tTypeToMarkupCode[tSupportData.sType]
    if sMarkupCode then
      sOutput = sMarkupCode .. " " .. sOutput
      bTypeIconPrepended = true
    end
  end
  local tFactionsStockingItem = {}
  local tFactionsWithShops = {
    "Pmc",
    "Oil",
    "Gur",
    "Pir",
    "All",
    "Chi"
  }
  for i, sFactionId in ipairs(tFactionsWithShops) do
    if MrxShop.GetNumberOfUnlockedItems(sFactionId) > 0 then
      local tSupport, tEquipment = MrxRewardData.GetAllPotentialShopItems(sFactionId)
      for i, sThisSupportId in ipairs(tSupport) do
        if sThisSupportId == sSupportId then
          table.insert(tFactionsStockingItem, sFactionId)
          break
        end
      end
    end
  end
  if 0 < #tFactionsStockingItem then
    sOutput = sOutput .. [[

[indent] [PDA.Map.RecommendationsSoldBy] ]]
    for i, sFactionId in ipairs(tFactionsStockingItem) do
      local sMarkupCode = MrxFactionManager.GetInlineIcon(sFactionId)
      sOutput = sOutput .. sMarkupCode
    end
  end
  local bInStock
  if nQty then
    local nStockedQty = MrxPmc.GetSupportQty(sSupportId) or 0
    local sCheckbox = "[check0]"
    bInStock = false
    if nQty <= nStockedQty then
      sCheckbox = "[check1]"
      bInStock = true
    end
    local sPrefix = sCheckbox .. " " .. nQty .. " x "
    if not bTypeIconPrepended then
      sPrefix = sPrefix .. " "
    end
    sOutput = sPrefix .. sOutput
  end
  return sOutput, bInStock
end
